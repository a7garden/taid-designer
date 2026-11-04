use super::*;

fn tile_settings() -> TileSettings {
    TileSettings {
        width: 16,
        height: 12,
        scale: 2,
        mode: TileMode::Pad,
        anchor: TileAnchor::BottomCenter,
    }
}
fn loaded(project: Project) -> EngineState {
    let state = EngineState {
        recent_directory: Some(project.root.join("app-data")),
        ..EngineState::default()
    };
    state.lock().unwrap().project = Some(project);
    state
}

#[test]
fn tile_creates_scoped_profile_preserves_parent_approval_and_has_independent_reset() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    project.document.profiles[0].stage = "NamedStage".into();
    project.document.profiles[0].palette = vec!["#5078A0".into()];
    project
        .update(
            &id,
            AssetPatch {
                sequence: Some("walk".into()),
                duration_ms: Some(135),
                pivot_x: Some(2.5),
                pivot_y: Some(6.5),
                layer: Some(-3),
                ..AssetPatch::default()
            },
        )
        .unwrap();
    project.refresh().unwrap();
    project.approve(&id, true, true).unwrap();
    project.persist().unwrap();
    let parent_profile = project.document.profiles[0].clone();
    let root = project.root.clone();
    let parent_file = fs::read(root.join(&project.document.assets[0].asset.file_path)).unwrap();
    let state = loaded(project);
    let result = state
        .tile_assets(std::slice::from_ref(&id), &tile_settings())
        .unwrap();
    let parent = &result.assets[0];
    let variant = &result.assets[1];
    assert_eq!(parent.id, id);
    assert!(parent.design_approved && parent.qc_approved);
    assert_eq!(fs::read(&parent.file_path).unwrap(), parent_file);
    assert_eq!(
        result
            .profiles
            .iter()
            .find(|profile| profile.id == parent.profile_id),
        Some(&parent_profile)
    );
    let profile = result
        .profiles
        .iter()
        .find(|profile| profile.id == variant.profile_id)
        .unwrap();
    assert_ne!(profile.id, parent.profile_id);
    assert_eq!(
        (profile.pixel_size, profile.width, profile.height),
        (2, Some(32), Some(24))
    );
    assert_eq!(
        (&profile.palette, &profile.alpha_mode, &profile.stage),
        (
            &parent_profile.palette,
            &parent_profile.alpha_mode,
            &parent_profile.stage
        )
    );
    assert_eq!((variant.width, variant.height), (32, 24));
    assert_eq!((variant.pivot_x, variant.pivot_y), (13.0, 21.0));
    assert_eq!((variant.duration_ms, variant.layer), (135, -3));
    assert_eq!(variant.sequence, "walk_tile");
    assert!(!variant.design_approved && !variant.qc_approved);
    assert!(!variant
        .issues
        .iter()
        .any(|issue| issue.code == "duplicate-frame"));
    let reset_base = pixels::load(Path::new(&variant.file_path)).unwrap();
    let mut guard = state.lock().unwrap();
    let candidate = guard.project.as_mut().unwrap();
    let mut options = settings();
    options.scale = 2;
    candidate
        .process(std::slice::from_ref(&variant.id), &options)
        .unwrap();
    candidate.reset(&variant.id).unwrap();
    let reset = &candidate.document.assets[candidate.index(&variant.id).unwrap()].asset;
    assert_eq!(
        (reset.pivot_x, reset.pivot_y),
        (variant.pivot_x, variant.pivot_y)
    );
    assert_eq!(
        pixels::load(&confined(&candidate.root, &reset.file_path).unwrap()).unwrap(),
        reset_base
    );
}

#[test]
fn palette_inspect_and_map_use_current_cutout_not_the_import_original() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    project.document.profiles[0].palette = vec!["#000000".into(), "#FFFFFF".into()];
    project.refresh().unwrap();
    project.persist().unwrap();
    let state = loaded(project);
    let snapshot = state.prepare_variants(std::slice::from_ref(&id)).unwrap();
    let mut cutout = snapshot.load_image(0).unwrap();
    cutout.get_pixel_mut(3, 4)[3] = 90;
    let staged = PathBuf::from(fixture.image("cutout-contract.png", &cutout));
    let cut_workspace = state
        .add_cutout_variants(&snapshot, &[staged], "contract", "r1")
        .unwrap();
    let cut_id = cut_workspace.assets[1].id.clone();
    let report = state.asset_palette(&cut_id).unwrap();
    assert_eq!(
        (
            report.visible_pixels,
            report.color_count,
            report.outside_pixels
        ),
        (1, 1, 1)
    );
    let result = state
        .map_palette_assets(std::slice::from_ref(&cut_id))
        .unwrap();
    let variant = &result.assets[2];
    let mapped = pixels::load(Path::new(&variant.file_path)).unwrap();
    assert_eq!(mapped.get_pixel(3, 4)[3], 90);
    assert_eq!(&mapped.get_pixel(3, 4).0[..3], &[0, 0, 0]);
    assert_eq!(
        pixels::load(Path::new(&result.assets[1].file_path)).unwrap(),
        cutout
    );
    assert!(!variant.design_approved && !variant.qc_approved);
    assert_ne!(variant.sequence, result.assets[1].sequence);
    assert_eq!(variant.profile_id, result.assets[1].profile_id);
    assert_eq!(variant.pivot_x, result.assets[1].pivot_x);
    assert_eq!(variant.pivot_y, result.assets[1].pivot_y);
}

#[test]
fn tile_batch_failure_leaves_no_profile_manifest_or_new_image() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let first = import_one(&fixture, &mut project);
    project
        .import(
            &[fixture.image("oversized.png", &RgbaImage::new(40, 40))],
            "generic",
        )
        .unwrap();
    project.refresh().unwrap();
    project.persist().unwrap();
    let root = project.root.clone();
    let second = project.document.assets[1].asset.id.clone();
    let manifest = fs::read(root.join(PROJECT_FILE)).unwrap();
    let count = fs::read_dir(root.join("originals")).unwrap().count();
    let profile_count = project.document.profiles.len();
    let state = loaded(project);
    assert!(state
        .tile_assets(&[first, second], &tile_settings())
        .is_err());
    assert_eq!(fs::read(root.join(PROJECT_FILE)).unwrap(), manifest);
    assert_eq!(fs::read_dir(root.join("originals")).unwrap().count(), count);
    let result = state.current_workspace().unwrap().unwrap();
    assert_eq!(result.assets.len(), 2);
    assert_eq!(result.profiles.len(), profile_count);
}

#[test]
fn palette_batch_missing_approved_palette_rolls_back_earlier_results() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let first = import_one(&fixture, &mut project);
    project.document.profiles[0].palette = vec!["#000000".into()];
    project
        .import(&[fixture.image("effect.png", &art())], "effect")
        .unwrap();
    project.refresh().unwrap();
    project.persist().unwrap();
    let second = project.document.assets[1].asset.id.clone();
    let root = project.root.clone();
    let manifest = fs::read(root.join(PROJECT_FILE)).unwrap();
    let count = fs::read_dir(root.join("originals")).unwrap().count();
    let state = loaded(project);
    assert!(state.map_palette_assets(&[first, second]).is_err());
    assert_eq!(fs::read(root.join(PROJECT_FILE)).unwrap(), manifest);
    assert_eq!(fs::read_dir(root.join("originals")).unwrap().count(), count);
    assert_eq!(state.current_workspace().unwrap().unwrap().assets.len(), 2);
}

#[test]
fn tile_inherits_reference_and_shares_only_new_profile_across_selected_frames() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    project
        .import(
            &[
                fixture.image("idle_00.png", &art()),
                fixture.image("idle_01.png", &art()),
            ],
            "generic",
        )
        .unwrap();
    let ids: Vec<_> = project
        .document
        .assets
        .iter()
        .map(|stored| stored.asset.id.clone())
        .collect();
    project
        .update(
            &ids[0],
            AssetPatch {
                reference: Some(true),
                ..AssetPatch::default()
            },
        )
        .unwrap();
    project.refresh().unwrap();
    project.persist().unwrap();
    let state = loaded(project);
    let result = state.tile_assets(&ids, &tile_settings()).unwrap();
    assert!(result.assets[2].reference);
    assert!(!result.assets[3].reference);
    assert_eq!(result.assets[2].sequence, result.assets[3].sequence);
    assert_ne!(result.assets[2].sequence, result.assets[0].sequence);
    assert_eq!(result.assets[2].profile_id, result.assets[3].profile_id);
    assert_eq!(result.profiles.len(), 5);
    assert_eq!(
        (result.assets[2].frame_index, result.assets[3].frame_index),
        (0, 1)
    );
    assert!(result.assets.iter().all(|asset| !asset
        .issues
        .iter()
        .any(|issue| issue.code == "duplicate-frame")));
}

#[test]
fn palette_file_import_changes_only_explicit_profile_and_invalidates_its_approval() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    project
        .import(&[fixture.image("untouched.png", &art())], "effect")
        .unwrap();
    project.refresh().unwrap();
    project.approve(&id, true, true).unwrap();
    let second = project.document.assets[1].asset.id.clone();
    project.approve(&second, true, true).unwrap();
    project.persist().unwrap();
    let mut profile = project.document.profiles[0].clone();
    let path = fixture.root.join("approved.hex");
    fs::write(&path, "5078A0\n000000\n").unwrap();
    profile.palette = read_palette_file(&path).unwrap();
    let state = loaded(project);
    let result = state.save_profile(profile).unwrap();
    assert!(!result.assets[0].design_approved && !result.assets[0].qc_approved);
    assert!(result.assets[1].design_approved && result.assets[1].qc_approved);
    assert_eq!(result.profiles[0].palette, ["#5078A0", "#000000"]);
    assert!(result
        .profiles
        .iter()
        .find(|profile| profile.id == "effect")
        .unwrap()
        .palette
        .is_empty());
}
