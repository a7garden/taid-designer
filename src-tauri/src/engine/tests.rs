use super::*;
use image::Rgba;

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("taid-engine-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self { root }
    }
    fn image(&self, name: &str, image: &RgbaImage) -> String {
        let path = self.root.join(name);
        image.save_with_format(&path, ImageFormat::Png).unwrap();
        path.to_string_lossy().into_owned()
    }
    fn project(&self) -> Project {
        Project::create(&self.root, "Test project").unwrap()
    }
    fn state(&self) -> EngineState {
        EngineState {
            recent_directory: Some(self.root.join("app-data")),
            ..EngineState::default()
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn art() -> RgbaImage {
    let mut image = RgbaImage::new(8, 8);
    image.put_pixel(3, 4, Rgba([80, 120, 160, 255]));
    image
}
fn settings() -> ProcessSettings {
    ProcessSettings {
        snap_to_grid: false,
        harden_alpha: false,
        map_palette: false,
        trim: false,
        scale: 1,
    }
}
fn import_one(f: &Fixture, project: &mut Project) -> String {
    project
        .import(&[f.image("hero.png", &art())], "generic")
        .unwrap();
    project.refresh().unwrap();
    project.document.assets[0].asset.id.clone()
}

#[test]
fn effect_profile_preserves_intentional_alpha() {
    let mut image = RgbaImage::new(2, 2);
    image.put_pixel(1, 1, Rgba([50, 100, 150, 100]));
    let profiles = default_profiles();
    let (issues, metrics) = pixels::inspect(&image, &profiles[3]).unwrap();
    assert_eq!(metrics.semi_transparent, 1);
    assert!(!issues.iter().any(|i| i.code == "alpha"));
    let mut binary = profiles[3].clone();
    binary.alpha_mode = "binary".into();
    assert!(pixels::inspect(&image, &binary)
        .unwrap()
        .0
        .iter()
        .any(|i| i.code == "alpha"));
}

#[test]
fn grid_ignores_hidden_rgb_but_detects_visible_variation() {
    let profile = &default_profiles()[1];
    let mut image = RgbaImage::from_pixel(4, 4, Rgba([30, 80, 200, 0]));
    image.put_pixel(1, 2, Rgba([240, 3, 12, 0]));
    let (_, metrics) = pixels::inspect(&image, profile).unwrap();
    assert_eq!(metrics.grid_violations, 0);
    image.put_pixel(1, 2, Rgba([240, 3, 12, 255]));
    let (issues, metrics) = pixels::inspect(&image, profile).unwrap();
    assert_eq!(metrics.grid_violations, 1);
    assert!(issues.iter().any(|i| i.code == "grid"));
}

#[test]
fn processing_and_reset_retain_immutable_original() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    let original = confined(
        &project.root,
        &project.document.assets[0].asset.original_path,
    )
    .unwrap();
    let before = fs::read(&original).unwrap();
    let mut options = settings();
    options.trim = true;
    options.scale = 3;
    project
        .process(std::slice::from_ref(&id), &options)
        .unwrap();
    project.refresh().unwrap();
    let asset = &project.document.assets[0].asset;
    assert_eq!((asset.width, asset.height), (3, 3));
    assert_eq!((asset.crop_x, asset.crop_y), (9, 12));
    assert_eq!((asset.pivot_x, asset.pivot_y), (3.0, 12.0));
    assert_eq!(before, fs::read(&original).unwrap());
    project
        .process(std::slice::from_ref(&id), &settings())
        .unwrap();
    assert_eq!(
        (
            project.document.assets[0].asset.width,
            project.document.assets[0].asset.height
        ),
        (8, 8)
    );
    project.reset(&id).unwrap();
    assert_eq!(
        project.document.assets[0].asset.file_path,
        project.document.assets[0].asset.original_path
    );
    assert_eq!(before, fs::read(original).unwrap());
}

#[test]
fn union_trim_requires_complete_sequence_and_preserves_motion() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let a = art();
    let mut b = RgbaImage::new(8, 8);
    b.put_pixel(5, 6, Rgba([80, 120, 160, 255]));
    project
        .import(
            &[
                fixture.image("idle0.png", &a),
                fixture.image("idle1.png", &b),
            ],
            "generic",
        )
        .unwrap();
    let ids: Vec<_> = project
        .document
        .assets
        .iter()
        .map(|a| a.asset.id.clone())
        .collect();
    for (index, id) in ids.iter().enumerate() {
        project
            .update(
                id,
                AssetPatch {
                    sequence: Some("idle".into()),
                    frame_index: Some(index as u32),
                    ..AssetPatch::default()
                },
            )
            .unwrap();
    }
    let mut options = settings();
    options.trim = true;
    assert!(project
        .process(&ids[..1], &options)
        .unwrap_err()
        .contains("모든 프레임"));
    project.process(&ids, &options).unwrap();
    project.refresh().unwrap();
    for stored in &project.document.assets {
        assert_eq!((stored.asset.width, stored.asset.height), (3, 3));
        assert_eq!((stored.asset.crop_x, stored.asset.crop_y), (3, 4));
    }
    let first = pixels::load(
        &confined(&project.root, &project.document.assets[0].asset.file_path).unwrap(),
    )
    .unwrap();
    let second = pixels::load(
        &confined(&project.root, &project.document.assets[1].asset.file_path).unwrap(),
    )
    .unwrap();
    assert_eq!(first.get_pixel(0, 0)[3], 255);
    assert_eq!(second.get_pixel(2, 2)[3], 255);
}

#[test]
fn union_trim_rejects_different_canvas_and_empty_art() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let a = art();
    let mut b = RgbaImage::new(9, 8);
    b.put_pixel(1, 1, Rgba([1, 2, 3, 255]));
    project
        .import(
            &[fixture.image("a.png", &a), fixture.image("b.png", &b)],
            "generic",
        )
        .unwrap();
    let ids: Vec<_> = project
        .document
        .assets
        .iter()
        .map(|a| a.asset.id.clone())
        .collect();
    for (i, id) in ids.iter().enumerate() {
        project
            .update(
                id,
                AssetPatch {
                    sequence: Some("walk".into()),
                    frame_index: Some(i as u32),
                    ..AssetPatch::default()
                },
            )
            .unwrap();
    }
    let mut options = settings();
    options.trim = true;
    assert!(project
        .process(&ids, &options)
        .unwrap_err()
        .contains("모두 같아야"));
    let empty = RgbaImage::new(8, 8);
    project
        .import(&[fixture.image("empty.png", &empty)], "generic")
        .unwrap();
    let empty_id = project.document.assets[2].asset.id.clone();
    assert!(project
        .process(&[empty_id], &options)
        .unwrap_err()
        .contains("그림이 없는"));
}

#[test]
fn approvals_are_separate_and_metadata_requires_new_qc() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    assert!(project.approve(&id, false, true).is_err());
    project.approve(&id, true, false).unwrap();
    assert!(project
        .export(&fixture.root, std::slice::from_ref(&id))
        .is_err());
    project.approve(&id, true, true).unwrap();
    project
        .update(
            &id,
            AssetPatch {
                duration_ms: Some(180),
                ..AssetPatch::default()
            },
        )
        .unwrap();
    project.refresh().unwrap();
    assert!(project.document.assets[0].asset.design_approved);
    assert!(!project.document.assets[0].asset.qc_approved);
    assert!(project
        .export(&fixture.root, std::slice::from_ref(&id))
        .is_err());
    project.approve(&id, true, true).unwrap();
    project.process(&[id], &settings()).unwrap();
    project.refresh().unwrap();
    assert!(!project.document.assets[0].asset.design_approved);
    assert!(!project.document.assets[0].asset.qc_approved);
}

#[test]
fn save_reopen_export_has_png_manifest_csv_and_handoff_only() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    project
        .update(
            &id,
            AssetPatch {
                notes: Some("Engine opacity, then move 12px.\nDo not bake \"glow\".".into()),
                ..AssetPatch::default()
            },
        )
        .unwrap();
    project.approve(&id, true, true).unwrap();
    project.persist().unwrap();
    let mut reopened = Project::open(&project.root).unwrap();
    reopened.refresh().unwrap();
    assert!(reopened.document.assets[0].asset.qc_approved);
    let export = reopened
        .export(&fixture.root, std::slice::from_ref(&id))
        .unwrap();
    let output = Path::new(&export.path);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["assets"][0]["pivot"]["origin"], "top-left");
    assert_eq!(manifest["assets"][0]["source"]["filename"], "hero.png");
    assert_eq!(fs::read_dir(output).unwrap().count(), 4);
    let mut reader = csv::Reader::from_path(output.join("assets.csv")).unwrap();
    let row = reader.records().next().unwrap().unwrap();
    assert_eq!(
        &row[16],
        "Engine opacity, then move 12px.\nDo not bake \"glow\"."
    );
    assert!(output.join("hero.png").exists());
    assert!(output.join("HANDOFF.md").exists());
    let next = reopened.export(&fixture.root, &[id]).unwrap();
    assert_ne!(export.path, next.path);
}

#[test]
fn source_and_import_name_collisions_never_overwrite() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let source = fixture.image("hero.png", &art());
    let bytes = fs::read(&source).unwrap();
    project
        .import(&[source.clone(), source.clone()], "generic")
        .unwrap();
    project.refresh().unwrap();
    assert_eq!(project.document.assets[0].asset.name, "hero");
    assert_eq!(project.document.assets[1].asset.name, "hero_2");
    assert_eq!(
        (
            project.document.assets[0].asset.frame_index,
            project.document.assets[1].asset.frame_index
        ),
        (0, 0)
    );
    assert!(project.document.assets.iter().all(|stored| stored
        .asset
        .issues
        .iter()
        .any(|issue| issue.code == "duplicate-frame")));
    assert_ne!(
        project.document.assets[0].asset.original_path,
        project.document.assets[1].asset.original_path
    );
    assert_eq!(bytes, fs::read(source).unwrap());
}

#[test]
fn empty_sequence_does_not_create_duplicate_frame_errors() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    project
        .import(
            &[
                fixture.image("a.png", &art()),
                fixture.image("b.png", &art()),
            ],
            "generic",
        )
        .unwrap();
    let ids: Vec<_> = project
        .document
        .assets
        .iter()
        .map(|a| a.asset.id.clone())
        .collect();
    for id in ids {
        project
            .update(
                &id,
                AssetPatch {
                    sequence: Some(String::new()),
                    frame_index: Some(0),
                    ..AssetPatch::default()
                },
            )
            .unwrap();
    }
    project.refresh().unwrap();
    assert!(project.document.assets.iter().all(|a| !a
        .asset
        .issues
        .iter()
        .any(|i| i.code == "duplicate-frame")));
}

#[test]
fn export_rejects_entire_invalid_or_reference_selection() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    project.approve(&id, true, true).unwrap();
    assert!(project
        .export(&fixture.root, &[id.clone(), "missing".into()])
        .is_err());
    assert!(project
        .export(&fixture.root, &[id.clone(), id.clone()])
        .is_err());
    project
        .update(
            &id,
            AssetPatch {
                reference: Some(true),
                ..AssetPatch::default()
            },
        )
        .unwrap();
    project.refresh().unwrap();
    assert!(project.approve(&id, true, true).is_err());
    assert!(project.export(&fixture.root, &[id]).is_err());
}

#[test]
fn unsafe_paths_and_names_are_rejected() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    assert!(project
        .update(
            &id,
            AssetPatch {
                name: Some("../escape".into()),
                ..AssetPatch::default()
            }
        )
        .is_err());
    assert!(project.index("../../outside").is_err());
    project.document.assets[0].asset.file_path = "processed/../../hero.png".into();
    assert!(project.refresh().is_err());
    assert!(confined(&project.root, "../hero.png").is_err());
}

#[cfg(unix)]
#[test]
fn symlink_artifact_escape_is_rejected() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let _id = import_one(&fixture, &mut project);
    let original = project
        .root
        .join(&project.document.assets[0].asset.original_path);
    fs::remove_file(&original).unwrap();
    std::os::unix::fs::symlink(fixture.root.join("hero.png"), &original).unwrap();
    assert!(project.refresh().unwrap_err().contains("작업 폴더 밖"));
}

#[test]
fn corrupt_manifest_does_not_replace_current_state() {
    let fixture = Fixture::new();
    let project = fixture.project();
    let state = fixture.state();
    state.lock().unwrap().project = Some(project.clone());
    fs::write(project.root.join(PROJECT_FILE), b"{not json}").unwrap();
    assert!(Project::open(&project.root).is_err());
    assert_eq!(
        state.lock().unwrap().project.as_ref().unwrap().root,
        project.root
    );
    assert!(state
        .mutate(|p| {
            p.document.name.clear();
            Ok(())
        })
        .is_err());
    assert_eq!(
        state
            .lock()
            .unwrap()
            .project
            .as_ref()
            .unwrap()
            .document
            .name,
        "Test project"
    );
}

#[test]
fn external_png_change_clears_approval_and_original_tamper_fails() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    project
        .process(std::slice::from_ref(&id), &settings())
        .unwrap();
    project.refresh().unwrap();
    project.approve(&id, true, true).unwrap();
    project.persist().unwrap();
    let current = project
        .root
        .join(&project.document.assets[0].asset.file_path);
    let mut changed = art();
    changed.put_pixel(0, 0, Rgba([1, 2, 3, 255]));
    changed.save(&current).unwrap();
    let reopened = Project::open(&project.root).unwrap();
    assert!(!reopened.document.assets[0].asset.design_approved);
    assert!(!reopened.document.assets[0].asset.qc_approved);
    assert!(reopened.export(&fixture.root, &[id]).is_err());
    let original = project
        .root
        .join(&project.document.assets[0].asset.original_path);
    changed.save(original).unwrap();
    assert!(Project::open(&project.root).is_err());
}

#[test]
fn rules_and_attachment_boundaries_hold() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    project.approve(&id, true, true).unwrap();
    project.document.profiles[0].pixel_size = 2;
    project.refresh().unwrap();
    assert!(!project.document.assets[0].asset.design_approved);
    assert!(!project.document.assets[0].asset.qc_approved);
    let original = project
        .root
        .join(&project.document.assets[0].asset.original_path)
        .to_string_lossy()
        .into_owned();
    let state = fixture.state();
    state.lock().unwrap().project = Some(project);
    assert_eq!(
        state
            .validate_image_paths(std::slice::from_ref(&original))
            .unwrap(),
        vec![original]
    );
    assert!(state
        .validate_image_paths(&[fixture.root.join("hero.png").to_string_lossy().into_owned()])
        .is_err());
    let mut profile = default_profiles()[0].clone();
    profile.palette = vec!["#GG0000".into()];
    assert!(validate_profile(&mut profile).is_err());
    profile.palette.clear();
    profile.pixel_size = 0;
    assert!(validate_profile(&mut profile).is_err());
    assert!(pixels::dimensions_valid(16384, 16384).is_err());
}

#[test]
fn alpha_threshold_zero_never_reveals_transparent_background() {
    let mut image = RgbaImage::new(3, 1);
    image.put_pixel(0, 0, Rgba([240, 80, 10, 0]));
    image.put_pixel(1, 0, Rgba([20, 50, 80, 1]));
    image.put_pixel(2, 0, Rgba([80, 120, 160, 255]));
    pixels::harden(&mut image, 0);
    assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 0]);
    assert_eq!(image.get_pixel(1, 0).0, [20, 50, 80, 255]);
    assert_eq!(image.get_pixel(2, 0).0, [80, 120, 160, 255]);
    let mut profile = default_profiles()[0].clone();
    profile.alpha_threshold = 0;
    assert!(validate_profile(&mut profile).is_err());
}

#[test]
fn csv_formula_text_is_inert_while_manifest_retains_source_and_notes() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let source_name = "=source.png";
    project
        .import(&[fixture.image(source_name, &art())], "generic")
        .unwrap();
    project.refresh().unwrap();
    let id = project.document.assets[0].asset.id.clone();
    let notes = "=HYPERLINK(\"https://example.invalid\",\"do not execute\")";
    let sequence = "@animation";
    project
        .update(
            &id,
            AssetPatch {
                notes: Some(notes.into()),
                sequence: Some(sequence.into()),
                ..AssetPatch::default()
            },
        )
        .unwrap();
    project.refresh().unwrap();
    project.approve(&id, true, true).unwrap();
    let exported = project.export(&fixture.root, &[id]).unwrap();
    let output = Path::new(&exported.path);
    let mut csv = csv::Reader::from_path(output.join("assets.csv")).unwrap();
    let row = csv.records().next().unwrap().unwrap();
    assert_eq!(&row[16], format!("'{notes}"));
    assert_eq!(&row[17], format!("'{source_name}"));
    assert_eq!(&row[4], format!("'{sequence}"));
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["assets"][0]["notes"], notes);
    assert_eq!(manifest["assets"][0]["sequence"], sequence);
    assert_eq!(manifest["assets"][0]["source"]["filename"], source_name);
    for risky in [
        "+SUM(1,2)",
        "-cmd",
        "@formula",
        "\tvalue",
        "\rvalue",
        "\nvalue",
        "  =formula",
    ] {
        assert_eq!(delivery::spreadsheet_text(risky), format!("'{risky}"));
    }
}

#[test]
fn inferred_frames_preserve_source_names_and_do_not_invent_movement() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    project
        .import(
            &[
                fixture.image("Player_idle_00.png", &art()),
                fixture.image("Player_idle-01.png", &art()),
                fixture.image("unrelated1.png", &art()),
            ],
            "generic",
        )
        .unwrap();
    project.refresh().unwrap();
    let assets = &project.document.assets;
    assert_eq!(assets[0].asset.source_name, "Player_idle_00.png");
    assert_eq!(assets[1].asset.source_name, "Player_idle-01.png");
    assert_eq!(assets[0].asset.sequence, "Player_idle");
    assert_eq!(assets[1].asset.sequence, "Player_idle");
    assert_eq!(assets[0].asset.frame_index, 0);
    assert_eq!(assets[1].asset.frame_index, 1);
    assert_eq!(assets[2].asset.sequence, "unrelated1");
    assert_eq!(assets[2].asset.frame_index, 0);
    assert!(assets
        .iter()
        .all(|asset| !asset.asset.design_approved && !asset.asset.qc_approved));
}

#[test]
fn external_metadata_revision_cannot_be_overwritten_by_loaded_state() {
    let fixture = Fixture::new();
    let project = fixture.project();
    let state = fixture.state();
    state.lock().unwrap().project = Some(project.clone());
    let mut external = Project::open(&project.root).unwrap();
    external.document.notes = "CLI edit".into();
    external.persist().unwrap();
    let bytes_before = fs::read(project.root.join(PROJECT_FILE)).unwrap();
    assert!(state.workspace_changed().unwrap());
    assert_eq!(state.current_workspace().unwrap().unwrap().notes, "");
    assert!(state
        .mutate(|candidate| {
            candidate.document.notes = "Stale GUI edit".into();
            Ok(())
        })
        .is_err());
    assert_eq!(
        fs::read(project.root.join(PROJECT_FILE)).unwrap(),
        bytes_before
    );
    assert_eq!(
        Project::open(&project.root).unwrap().document.notes,
        "CLI edit"
    );
    assert_eq!(state.current_workspace().unwrap().unwrap().notes, "");
}

#[test]
fn independent_workspace_handles_cannot_write_while_lock_is_held() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let lock = WorkspaceLock::acquire(&project.root).unwrap();
    assert!(WorkspaceLock::acquire(&project.root).is_err());
    assert!(Project::open(&project.root).is_err());
    project.document.notes = "Pending edit".into();
    assert!(project.persist().is_err());
    drop(lock);
    project.persist().unwrap();
    assert_eq!(
        Project::open(&project.root).unwrap().document.notes,
        "Pending edit"
    );
}

#[test]
fn image_changed_after_preview_is_not_sent_as_the_reviewed_version() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    import_one(&fixture, &mut project);
    let path = project
        .root
        .join(&project.document.assets[0].asset.file_path);
    let attachment = path.to_string_lossy().into_owned();
    let state = fixture.state();
    state.lock().unwrap().project = Some(project);
    assert!(state
        .validate_image_paths(std::slice::from_ref(&attachment))
        .is_ok());
    let mut changed = art();
    changed.put_pixel(0, 0, Rgba([240, 30, 20, 255]));
    changed.save(&path).unwrap();
    assert!(state.validate_image_paths(&[attachment]).is_err());
}

#[test]
fn cutout_variants_keep_current_canvas_metadata_and_independent_reset_base() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    project
        .update(
            &id,
            AssetPatch {
                sequence: Some("idle".into()),
                duration_ms: Some(175),
                pivot_x: Some(2.5),
                pivot_y: Some(6.5),
                layer: Some(7),
                reference: Some(true),
                ..AssetPatch::default()
            },
        )
        .unwrap();
    let mut options = settings();
    options.trim = true;
    options.scale = 2;
    project
        .process(std::slice::from_ref(&id), &options)
        .unwrap();
    project.refresh().unwrap();
    project.persist().unwrap();
    let state = fixture.state();
    state.lock().unwrap().project = Some(project);
    let snapshot = state.prepare_variants(std::slice::from_ref(&id)).unwrap();
    let parent = snapshot.assets[0].clone();
    let original_before = fs::read(&parent.original_path).unwrap();
    let current_before = fs::read(&parent.file_path).unwrap();
    let mut derivative = snapshot.load_image(0).unwrap();
    for pixel in derivative.pixels_mut() {
        pixel[3] = ((u16::from(pixel[3]) * 128 + 127) / 255) as u8;
    }
    let staged = PathBuf::from(fixture.image("cutout.png", &derivative));
    let workspace = state
        .add_cutout_variants(&snapshot, &[staged], "test-contract", "revision-1")
        .unwrap();
    assert_eq!(workspace.assets.len(), 2);
    let variant = &workspace.assets[1];
    assert_ne!(variant.id, parent.id);
    assert_eq!(
        (variant.width, variant.height),
        (parent.width, parent.height)
    );
    assert_eq!(
        (variant.pivot_x, variant.pivot_y),
        (parent.pivot_x, parent.pivot_y)
    );
    assert_eq!(variant.duration_ms, 175);
    assert_eq!(variant.layer, 7);
    assert!(variant.reference);
    assert_eq!(variant.profile_id, parent.profile_id);
    assert_eq!(variant.sequence, "idle_cutout");
    assert!(!variant
        .issues
        .iter()
        .any(|issue| issue.code == "duplicate-frame"));
    assert!(!variant.design_approved && !variant.qc_approved && !variant.processed);
    assert!(variant.notes.contains(&parent.original_hash));
    assert!(variant.notes.contains(&parent.current_hash));
    assert!(variant.notes.contains("revision-1"));
    assert_eq!(fs::read(&parent.original_path).unwrap(), original_before);
    assert_eq!(fs::read(&parent.file_path).unwrap(), current_before);
    assert_eq!(
        pixels::load(Path::new(&variant.original_path)).unwrap(),
        derivative
    );
    let mut guard = state.lock().unwrap();
    let candidate = guard.project.as_mut().unwrap();
    let mut scale = settings();
    scale.scale = 2;
    candidate
        .process(std::slice::from_ref(&variant.id), &scale)
        .unwrap();
    candidate.reset(&variant.id).unwrap();
    let reset = &candidate.document.assets[candidate.index(&variant.id).unwrap()].asset;
    assert_eq!(
        pixels::load(&confined(&candidate.root, &reset.file_path).unwrap()).unwrap(),
        derivative
    );
    assert_eq!(
        (reset.pivot_x, reset.pivot_y),
        (variant.pivot_x, variant.pivot_y)
    );
}

#[test]
fn cutout_batch_failure_removes_new_files_without_committing_or_overwriting() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let first = import_one(&fixture, &mut project);
    project
        .import(&[fixture.image("second.png", &art())], "generic")
        .unwrap();
    project.refresh().unwrap();
    project.persist().unwrap();
    let second = project.document.assets[1].asset.id.clone();
    let root = project.root.clone();
    let manifest = fs::read(root.join(PROJECT_FILE)).unwrap();
    let count = fs::read_dir(root.join("originals")).unwrap().count();
    let state = fixture.state();
    state.lock().unwrap().project = Some(project);
    let snapshot = state.prepare_variants(&[first, second]).unwrap();
    let first_png = PathBuf::from(fixture.image("good.png", &snapshot.load_image(0).unwrap()));
    let wrong_canvas = PathBuf::from(fixture.image("wrong.png", &RgbaImage::new(2, 2)));
    assert!(state
        .add_cutout_variants(
            &snapshot,
            &[first_png.clone(), wrong_canvas],
            "contract",
            "r1"
        )
        .is_err());
    let mut opaque_fill = snapshot.load_image(1).unwrap();
    opaque_fill.get_pixel_mut(0, 0)[3] = 255;
    let invalid_alpha = PathBuf::from(fixture.image("opaque-fill.png", &opaque_fill));
    assert!(state
        .add_cutout_variants(
            &snapshot,
            &[first_png.clone(), invalid_alpha],
            "contract",
            "r1"
        )
        .is_err());
    let mut recolored = snapshot.load_image(1).unwrap();
    recolored.get_pixel_mut(3, 4)[0] = 1;
    let invalid_rgb = PathBuf::from(fixture.image("recolored.png", &recolored));
    assert!(state
        .add_cutout_variants(&snapshot, &[first_png, invalid_rgb], "contract", "r1")
        .is_err());
    assert_eq!(fs::read(root.join(PROJECT_FILE)).unwrap(), manifest);
    assert_eq!(fs::read_dir(root.join("originals")).unwrap().count(), count);
    assert_eq!(state.current_workspace().unwrap().unwrap().assets.len(), 2);
}

#[test]
fn cutout_rejects_stale_parent_hash_and_external_manifest_revision() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let id = import_one(&fixture, &mut project);
    project.persist().unwrap();
    let root = project.root.clone();
    let state = fixture.state();
    state.lock().unwrap().project = Some(project);
    let snapshot = state.prepare_variants(std::slice::from_ref(&id)).unwrap();
    let staged = PathBuf::from(fixture.image("prepared.png", &snapshot.load_image(0).unwrap()));
    let count = fs::read_dir(root.join("originals")).unwrap().count();
    let mut external = Project::open(&root).unwrap();
    external.document.notes = "Changed while inference ran".into();
    external.persist().unwrap();
    let external_manifest = fs::read(root.join(PROJECT_FILE)).unwrap();
    assert!(state
        .add_cutout_variants(&snapshot, std::slice::from_ref(&staged), "contract", "r1")
        .is_err());
    assert_eq!(
        fs::read(root.join(PROJECT_FILE)).unwrap(),
        external_manifest
    );
    assert_eq!(fs::read_dir(root.join("originals")).unwrap().count(), count);
    state.lock().unwrap().project = Some(Project::open(&root).unwrap());
    let fresh = state.prepare_variants(std::slice::from_ref(&id)).unwrap();
    // Mutate a current processed file, not the immutable original.
    {
        let mut guard = state.lock().unwrap();
        let candidate = guard.project.as_mut().unwrap();
        candidate
            .process(std::slice::from_ref(&id), &settings())
            .unwrap();
        candidate.refresh().unwrap();
        candidate.persist().unwrap();
    }
    let processed = state.prepare_variants(std::slice::from_ref(&id)).unwrap();
    let mut changed = processed.load_image(0).unwrap();
    changed.put_pixel(0, 0, Rgba([1, 2, 3, 255]));
    changed
        .save_with_format(&processed.assets[0].file_path, ImageFormat::Png)
        .unwrap();
    assert!(state
        .add_cutout_variants(&processed, std::slice::from_ref(&staged), "contract", "r1")
        .is_err());
    assert!(state
        .add_cutout_variants(&fresh, &[staged], "contract", "r1")
        .is_err());
    assert_eq!(fs::read_dir(root.join("originals")).unwrap().count(), count);
}

#[path = "palette_tile_tests.rs"]
mod palette_tile_tests;
