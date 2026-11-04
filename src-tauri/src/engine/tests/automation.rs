use super::*;

fn setup(
    image: &RgbaImage,
    grid: u32,
    width: Option<u32>,
    height: Option<u32>,
    alpha: &str,
    palette: &[&str],
) -> (Fixture, EngineState, String) {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let profile = project
        .document
        .profiles
        .iter_mut()
        .find(|p| p.id == "generic")
        .unwrap();
    profile.pixel_size = grid;
    profile.width = width;
    profile.height = height;
    profile.alpha_mode = alpha.into();
    profile.palette = palette.iter().map(|color| (*color).into()).collect();
    let profile_id = profile.id.clone();
    project
        .import(&[fixture.image("input.png", image)], &profile_id)
        .unwrap();
    project.refresh().unwrap();
    project.persist().unwrap();
    let id = project.document.assets[0].asset.id.clone();
    let state = fixture.state();
    state.lock().unwrap().project = Some(project);
    (fixture, state, id)
}

#[test]
fn preparation_corrects_actual_grid_alpha_palette_and_canvas_preserving_parent() {
    let mut image = RgbaImage::from_pixel(5, 3, Rgba([200, 20, 10, 200]));
    image.put_pixel(1, 1, Rgba([10, 90, 130, 255]));
    let (_fixture, state, id) = setup(&image, 2, Some(8), Some(8), "binary", &["#FF0000"]);
    state
        .approve_assets(std::slice::from_ref(&id), true, false)
        .unwrap();
    let before = state.current_workspace().unwrap().unwrap();
    let parent_before = serde_json::to_value(&before.assets[0]).unwrap();
    let png_before = fs::read(&before.assets[0].file_path).unwrap();
    let manifest_before = fs::read(Path::new(&before.root).join(PROJECT_FILE)).unwrap();
    let plan = state.automation_plan(std::slice::from_ref(&id)).unwrap();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    assert_eq!(plan.changed_count, 1);
    assert_eq!(plan.items[0].steps.len(), 5);
    assert_eq!(
        (plan.items[0].output_width, plan.items[0].output_height),
        (8, 8)
    );
    assert!(plan.items[0]
        .warnings
        .iter()
        .any(|warning| warning.contains("사라질")));
    assert_eq!(
        fs::read(Path::new(&before.root).join(PROJECT_FILE)).unwrap(),
        manifest_before
    );
    let after = state.auto_prepare(std::slice::from_ref(&id)).unwrap();
    assert_eq!(after.assets.len(), 2);
    assert_eq!(
        serde_json::to_value(&after.assets[0]).unwrap(),
        parent_before
    );
    assert_eq!(fs::read(&after.assets[0].file_path).unwrap(), png_before);
    let variant = &after.assets[1];
    assert_eq!(variant.parent_id.as_deref(), Some(id.as_str()));
    assert_eq!(
        variant.parent_hash.as_deref(),
        Some(before.assets[0].current_hash.as_str())
    );
    assert!(!variant.design_approved && !variant.qc_approved);
    assert!(!variant.issues.iter().any(|issue| issue.severity == "error"));
    let output = pixels::load(Path::new(&variant.file_path)).unwrap();
    assert_eq!(output.dimensions(), (8, 8));
    assert!(output
        .pixels()
        .all(|pixel| pixel[3] == 0 || pixel.0 == [255, 0, 0, 255]));
    assert_eq!(variant.metrics.grid_violations, 0);
    let second = state
        .auto_prepare(std::slice::from_ref(&variant.id))
        .unwrap();
    assert_eq!(second.assets.len(), 2, "conforming result must not clone");
}

#[test]
fn preparation_preserves_effect_alpha_and_noop_parent_approvals() {
    let image = RgbaImage::from_pixel(4, 4, Rgba([120, 20, 10, 91]));
    let (_fixture, state, id) = setup(&image, 2, Some(8), None, "preserve", &[]);
    let after = state.auto_prepare(std::slice::from_ref(&id)).unwrap();
    let output = pixels::load(Path::new(&after.assets[1].file_path)).unwrap();
    assert_eq!(
        output.dimensions(),
        (8, 4),
        "unspecified axis retains aligned source size"
    );
    assert!(output.pixels().all(|pixel| pixel[3] == 0 || pixel[3] == 91));
    let variant_id = after.assets[1].id.clone();
    state
        .approve_assets(std::slice::from_ref(&variant_id), true, true)
        .unwrap();
    let before = state.current_workspace().unwrap().unwrap();
    let unchanged = state
        .auto_prepare(std::slice::from_ref(&variant_id))
        .unwrap();
    assert_eq!(
        serde_json::to_value(&unchanged.assets).unwrap(),
        serde_json::to_value(&before.assets).unwrap()
    );
}

#[test]
fn preparation_pads_top_left_without_moving_pivot() {
    let image = RgbaImage::from_pixel(3, 3, Rgba([80, 100, 120, 255]));
    let (_fixture, state, id) = setup(&image, 2, None, None, "binary", &[]);
    let before = state.current_workspace().unwrap().unwrap();
    let after = state.auto_prepare(std::slice::from_ref(&id)).unwrap();
    assert_eq!((after.assets[1].width, after.assets[1].height), (4, 4));
    assert_eq!(
        (after.assets[1].pivot_x, after.assets[1].pivot_y),
        (before.assets[0].pivot_x, before.assets[0].pivot_y)
    );
    let output = pixels::load(Path::new(&after.assets[1].file_path)).unwrap();
    assert_eq!(*output.get_pixel(0, 0), *image.get_pixel(0, 0));
    assert_eq!(after.assets[1].metrics.grid_violations, 0);
}

#[test]
fn impossible_rule_or_reference_blocks_entire_batch_without_files() {
    let image = RgbaImage::from_pixel(4, 4, Rgba([100, 40, 10, 255]));
    let (fixture, state, first) = setup(&image, 2, None, None, "binary", &["#FF0000"]);
    let before = state.current_workspace().unwrap().unwrap();
    let second = state
        .import_assets(&[fixture.image("second.png", &image)], "generic")
        .unwrap()
        .assets[1]
        .id
        .clone();
    state
        .update_asset(
            &second,
            AssetPatch {
                reference: Some(true),
                ..AssetPatch::default()
            },
        )
        .unwrap();
    let root = Path::new(&before.root);
    let manifest = fs::read(root.join(PROJECT_FILE)).unwrap();
    let files = fs::read_dir(root.join("originals")).unwrap().count();
    let ids = [first.clone(), second];
    assert!(!state.automation_plan(&ids).unwrap().blocked.is_empty());
    assert!(state.auto_prepare(&ids).is_err());
    assert_eq!(fs::read(root.join(PROJECT_FILE)).unwrap(), manifest);
    assert_eq!(fs::read_dir(root.join("originals")).unwrap().count(), files);
    let mut profile = state
        .current_workspace()
        .unwrap()
        .unwrap()
        .profiles
        .into_iter()
        .find(|p| p.id == "generic")
        .unwrap();
    profile.width = Some(5);
    state.save_profile(profile).unwrap();
    let manifest = fs::read(root.join(PROJECT_FILE)).unwrap();
    assert!(!state
        .automation_plan(std::slice::from_ref(&first))
        .unwrap()
        .blocked
        .is_empty());
    assert!(state.auto_prepare(&[first]).is_err());
    assert_eq!(fs::read(root.join(PROJECT_FILE)).unwrap(), manifest);
    assert_eq!(fs::read_dir(root.join("originals")).unwrap().count(), files);
}

#[test]
fn preparation_blocks_stale_current_source_and_empty_loss_result() {
    let image = RgbaImage::from_pixel(4, 4, Rgba([100, 40, 10, 255]));
    let (_fixture, state, id) = setup(&image, 2, None, None, "binary", &[]);
    // A separate current PNG permits source mutation without tampering originals.
    state
        .process_assets(std::slice::from_ref(&id), &settings())
        .unwrap();
    let before = state.current_workspace().unwrap().unwrap();
    let root = Path::new(&before.root);
    let manifest = fs::read(root.join(PROJECT_FILE)).unwrap();
    let files = fs::read_dir(root.join("originals")).unwrap().count();
    let changed = RgbaImage::from_pixel(4, 4, Rgba([4, 5, 6, 255]));
    changed
        .save_with_format(&before.assets[0].file_path, ImageFormat::Png)
        .unwrap();
    assert!(!state
        .automation_plan(std::slice::from_ref(&id))
        .unwrap()
        .blocked
        .is_empty());
    assert!(state.auto_prepare(&[id]).is_err());
    assert_eq!(fs::read(root.join(PROJECT_FILE)).unwrap(), manifest);
    assert_eq!(fs::read_dir(root.join("originals")).unwrap().count(), files);

    let faint = RgbaImage::from_pixel(2, 2, Rgba([100, 40, 10, 1]));
    let (_fixture, state, id) = setup(&faint, 1, None, None, "binary", &[]);
    assert!(!state
        .automation_plan(std::slice::from_ref(&id))
        .unwrap()
        .blocked
        .is_empty());
    assert!(
        state.auto_prepare(&[id]).is_err(),
        "alpha loss must not create an empty ready variant"
    );
}

#[test]
fn preparation_reports_missing_profile_and_old_assets_load_without_parent_fields() {
    let image = RgbaImage::from_pixel(2, 2, Rgba([100, 40, 10, 255]));
    let (_fixture, state, id) = setup(&image, 1, None, None, "binary", &[]);
    let mut value =
        serde_json::to_value(&state.current_workspace().unwrap().unwrap().assets[0]).unwrap();
    value.as_object_mut().unwrap().remove("parentId");
    value.as_object_mut().unwrap().remove("parentHash");
    let legacy: Asset = serde_json::from_value(value).unwrap();
    assert!(legacy.parent_id.is_none() && legacy.parent_hash.is_none());
    {
        let mut guard = state.lock().unwrap();
        guard.project.as_mut().unwrap().document.profiles.clear();
    }
    assert!(!state
        .automation_plan(std::slice::from_ref(&id))
        .unwrap()
        .blocked
        .is_empty());
    assert!(state.auto_prepare(&[id]).is_err());
}

#[test]
fn preparation_rolls_back_prior_outputs_when_later_alpha_loss_empties_image() {
    let image = RgbaImage::from_pixel(4, 4, Rgba([100, 40, 10, 255]));
    let (fixture, state, first) = setup(&image, 2, None, None, "binary", &["#FF0000"]);
    let faint = RgbaImage::from_pixel(4, 4, Rgba([100, 40, 10, 1]));
    let workspace = state
        .import_assets(&[fixture.image("faint.png", &faint)], "generic")
        .unwrap();
    let second = workspace.assets[1].id.clone();
    let root = Path::new(&workspace.root);
    let manifest = fs::read(root.join(PROJECT_FILE)).unwrap();
    let files = fs::read_dir(root.join("originals")).unwrap().count();
    assert!(state.auto_prepare(&[first, second]).is_err());
    assert_eq!(fs::read(root.join(PROJECT_FILE)).unwrap(), manifest);
    assert_eq!(fs::read_dir(root.join("originals")).unwrap().count(), files);
    assert_eq!(state.current_workspace().unwrap().unwrap().assets.len(), 2);
}
