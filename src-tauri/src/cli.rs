use crate::{
    engine::{
        self, AssetPatch, EngineState, ProcessSettings, RuleProfile, TileAnchor, TileMode,
        TileSettings, Workspace,
    },
    settings,
};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "taid",
    version,
    about = "Local game asset workbench; JSON results, no GUI required",
    long_about = "Taid shares the desktop engine, immutable originals, QC and export approval gates. Processing flags are opt-in. Use an explicit --project for asset operations. Successful commands print one JSON envelope; failures print one JSON envelope to stderr and exit 1."
)]
struct Arguments {
    #[arg(
        long,
        global = true,
        value_name = "DIR",
        help = "Taid project folder or taid-project.json"
    )]
    project: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create, select or list projects.
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Inspect profiles, asset IDs, QC and approval state without image bytes.
    Inspect,
    /// Copy source images into immutable originals. _NN/-NN suffixes infer frames.
    Import {
        #[arg(required = true, value_name = "PATH", num_args = 1..)]
        paths: Vec<PathBuf>,
        #[arg(long, value_name = "ID")]
        profile: String,
    },
    /// Inspect or replace a complete rule profile using camelCase JSON.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Edit one asset's metadata using a camelCase JSON patch.
    Asset {
        #[command(subcommand)]
        command: AssetCommand,
    },
    /// Process from preserved originals; all changes are explicitly opt-in.
    Process {
        #[arg(long, required = true, value_delimiter = ',', value_name = "ID,...")]
        ids: Vec<String>,
        #[arg(long, help = "Snap pixels to the selected profile's grid")]
        snap: bool,
        #[arg(
            long,
            help = "Harden alpha; unavailable for preserve-alpha effect profiles"
        )]
        alpha: bool,
        #[arg(long, help = "Map colors to the selected profile's palette")]
        palette: bool,
        #[arg(
            long,
            help = "Union-trim a complete selected sequence, preserving motion offsets"
        )]
        trim: bool,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=8))]
        scale: u32,
    },
    /// Local U2Netp neural cutout into new unapproved variants. First use downloads
    /// 4.4 MiB Apache-2.0 weights; images never leave this machine.
    Cutout {
        #[arg(long, required = true, value_delimiter = ',', value_name = "ID,...")]
        ids: Vec<String>,
    },
    /// Inspect actual current-image RGB, import a real palette, or create mapped variants.
    Palette {
        #[command(subcommand)]
        command: PaletteCommand,
    },
    /// Create unapproved tile variants from CURRENT images, never overwriting parents.
    /// Logical canvas width/height times scale gives physical PNG dimensions.
    Tile {
        #[arg(long, required = true, value_delimiter = ',', value_name = "ID,...")]
        ids: Vec<String>,
        #[arg(long)]
        width: u32,
        #[arg(long)]
        height: u32,
        #[arg(long, default_value_t = 1)]
        scale: u32,
        #[arg(long, default_value = "pad", value_parser = ["pad", "fit"])]
        mode: String,
        #[arg(long, default_value = "center", value_parser = ["center", "bottom-center"])]
        anchor: String,
    },
    /// Restore selected assets from immutable originals; clears approval.
    Reset {
        #[arg(long, required = true, value_delimiter = ',', value_name = "ID,...")]
        ids: Vec<String>,
    },
    /// Record explicit human review. Never infer artistic approval from QC.
    Approve {
        #[arg(long, required = true, value_delimiter = ',', value_name = "ID,...")]
        ids: Vec<String>,
        #[arg(
            long,
            required = true,
            help = "The user explicitly requested design approval for these assets"
        )]
        design_reviewed: bool,
        #[arg(
            long,
            requires = "design_reviewed",
            help = "The user explicitly requested QC approval for these assets"
        )]
        qc_reviewed: bool,
    },
    /// Export approved non-reference assets into a new atomic delivery folder.
    Export {
        #[arg(long, required = true, value_delimiter = ',', value_name = "ID,...")]
        ids: Vec<String>,
        #[arg(
            long,
            value_name = "DIR",
            help = "Existing parent folder; a new delivery folder is created"
        )]
        out: PathBuf,
    },
    /// Replace project notes; metadata changes require renewed QC approval.
    Notes {
        #[arg(long, allow_hyphen_values = true)]
        text: String,
    },
    /// Read or update the same local Codex settings used by the desktop.
    Settings {
        #[command(subcommand)]
        command: SettingsCommand,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    Create {
        #[arg(long, value_name = "DIR")]
        parent: PathBuf,
        #[arg(long)]
        name: String,
    },
    /// Select an existing project; the next GUI launch opens this selection.
    Open {
        #[arg(value_name = "DIR")]
        path: PathBuf,
    },
    /// List the last 20 selections, including unavailable paths.
    Recent,
}

#[derive(Subcommand)]
enum ProfileCommand {
    List,
    Put {
        #[arg(long, value_name = "JSON")]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum AssetCommand {
    Update {
        #[arg(value_name = "ID")]
        id: String,
        #[arg(long, value_name = "PATCH.json")]
        file: PathBuf,
    },
}
#[derive(Subcommand)]
enum PaletteCommand {
    /// Read the current visible RGB histogram without modifying image or profile.
    Inspect {
        #[arg(long, value_name = "ID")]
        asset: String,
    },
    /// Explicitly replace one saved profile's approved palette; invalidates affected approvals.
    Import {
        #[arg(long, value_name = "PATH.hex|PATH.gpl")]
        file: PathBuf,
        #[arg(long, value_name = "ID")]
        profile: String,
    },
    /// Map CURRENT images to their saved profiles' palettes as new unapproved variants.
    Map {
        #[arg(long, required = true, value_delimiter = ',', value_name = "ID,...")]
        ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum SettingsCommand {
    Get,
    #[command(group(clap::ArgGroup::new("patch").required(true).multiple(true).args(["model", "effort"])))]
    Set {
        #[arg(
            long,
            help = "Codex model ID; use the desktop model list to discover available models"
        )]
        model: Option<String>,
        #[arg(long, value_parser = ["none", "minimal", "low", "medium", "high", "xhigh"])]
        effort: Option<String>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    status: &'static str,
    summary: String,
    data: Value,
    artifacts: Vec<String>,
    next_actions: Vec<String>,
}

fn success(
    summary: &str,
    data: impl Serialize,
    artifacts: Vec<String>,
    next_actions: &[&str],
) -> Result<Envelope, String> {
    Ok(Envelope {
        status: "success",
        summary: summary.into(),
        data: serde_json::to_value(data)
            .map_err(|error| format!("Cannot serialize result: {error}"))?,
        artifacts,
        next_actions: next_actions.iter().map(|value| (*value).into()).collect(),
    })
}

fn workspace_result(summary: &str, workspace: Workspace) -> Result<Envelope, String> {
    let root = workspace.root.clone();
    success(
        summary,
        workspace,
        vec![root],
        &["Inspect asset IDs, issues and approval state before the next operation."],
    )
}

fn read_input<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err("JSON input must be a regular file no larger than 16 MiB.".into());
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid JSON in {}: {error}", path.display()))
}

fn execute(arguments: Arguments) -> Result<Envelope, String> {
    let engine = EngineState::default();
    let command = match arguments.command {
        Command::Settings { command } => {
            return match command {
                SettingsCommand::Get => {
                    success("Local settings", settings::load()?, Vec::new(), &[])
                }
                SettingsCommand::Set { model, effort } => success(
                    "Local settings saved",
                    settings::update(model, effort)?,
                    Vec::new(),
                    &[],
                ),
            }
        }
        Command::Project { command } => {
            return match command {
                ProjectCommand::Create { parent, name } => workspace_result(
                    "Project created and selected",
                    engine.create_project(&parent, &name)?,
                ),
                ProjectCommand::Open { path } => {
                    workspace_result("Project opened and selected", engine.open_project(&path)?)
                }
                ProjectCommand::Recent => success(
                    "Recent projects; unavailable paths are retained",
                    engine::recent_projects()?,
                    Vec::new(),
                    &["Use taid project open DIR to select a project."],
                ),
            }
        }
        command => command,
    };
    let path = arguments.project.ok_or("An explicit --project DIR is required. Use taid project recent to find a project; stop if the intended project is unclear.")?;
    let workspace = engine.open_project(&path)?;
    match command {
        Command::Inspect => workspace_result("Loaded project snapshot", workspace),
        Command::Import { paths, profile } => {
            let paths = paths
                .iter()
                .map(|path| {
                    path.to_str()
                        .map(str::to_owned)
                        .ok_or("Source image paths must be valid UTF-8.".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            workspace_result(
                "Images imported; no design approval was granted",
                engine.import_assets(&paths, &profile)?,
            )
        }
        Command::Profile { command } => match command {
            ProfileCommand::List => success(
                "Project rule profiles",
                workspace.profiles,
                vec![workspace.root],
                &[],
            ),
            ProfileCommand::Put { file } => workspace_result(
                "Rule profile saved; affected approvals invalidated",
                engine.save_profile(read_input::<RuleProfile>(&file)?)?,
            ),
        },
        Command::Asset {
            command: AssetCommand::Update { id, file },
        } => workspace_result(
            "Asset metadata saved",
            engine.update_asset(&id, read_input::<AssetPatch>(&file)?)?,
        ),
        Command::Process {
            ids,
            snap,
            alpha,
            palette,
            trim,
            scale,
        } => workspace_result(
            "Selected assets processed from immutable originals",
            engine.process_assets(
                &ids,
                &ProcessSettings {
                    snap_to_grid: snap,
                    harden_alpha: alpha,
                    map_palette: palette,
                    trim,
                    scale,
                },
            )?,
        ),
        Command::Cutout { ids } => workspace_result(
            "Local cutout variants created; originals and approvals unchanged",
            crate::cutout::remove(
                &engine,
                &ids,
                &crate::cutout::CutoutState::default(),
                |event| {
                    eprintln!("{}", serde_json::json!({
                        "event": "cutout-progress", "data": event
                    }));
                },
            )?,
        ),
        Command::Palette { command } => match command {
            PaletteCommand::Inspect { asset } => success(
                "Current visible RGB palette inspected; no files changed",
                engine.asset_palette(&asset)?, Vec::new(), &[],
            ),
            PaletteCommand::Import { file, profile } => {
                let mut saved = workspace.profiles.into_iter().find(|candidate| candidate.id == profile)
                    .ok_or_else(|| format!("Profile not found: {profile}"))?;
                saved.palette = engine::read_palette_file(&file)?;
                workspace_result("Approved palette saved to the selected profile; affected approvals invalidated", engine.save_profile(saved)?)
            }
            PaletteCommand::Map { ids } => workspace_result(
                "Palette variants created from current images; parents preserved and new variants unapproved",
                engine.map_palette_assets(&ids)?,
            ),
        },
        Command::Tile { ids, width, height, scale, mode, anchor } => workspace_result(
            "Tile variants and scoped profiles created; parents preserved and new variants unapproved",
            engine.tile_assets(&ids, &TileSettings {
                width, height, scale,
                mode: if mode == "fit" { TileMode::Fit } else { TileMode::Pad },
                anchor: if anchor == "bottom-center" { TileAnchor::BottomCenter } else { TileAnchor::Center },
            })?,
        ),
        Command::Reset { ids } => workspace_result(
            "Selected assets reset; approvals cleared",
            engine.reset_assets(&ids)?,
        ),
        Command::Approve {
            ids,
            design_reviewed,
            qc_reviewed,
        } => {
            if !design_reviewed {
                return Err("Design approval requires an explicit --design-reviewed flag and the user's direct authorization. Stop rather than inferring artistic approval.".into());
            }
            workspace_result(
                "Explicit review recorded for selected assets",
                engine.approve_assets(&ids, design_reviewed, qc_reviewed)?,
            )
        }
        Command::Export { ids, out } => {
            let result = engine.export_assets(&out, &ids)?;
            let artifact = result.path.clone();
            success("Approved assets exported; atlas and Unity .meta remain outside Taid", result, vec![artifact], &["Inspect the delivery manifest and handoff notes before downstream integration."])
        }
        Command::Notes { text } => workspace_result(
            "Project notes saved; QC approval invalidated when changed",
            engine.save_notes(text)?,
        ),
        Command::Settings { .. } | Command::Project { .. } => {
            unreachable!("handled before project selection")
        }
    }
}

fn error_envelope(summary: String) -> Envelope {
    Envelope {
        status: "error", summary, data: Value::Null, artifacts: Vec::new(),
        next_actions: vec![
            "Correct the reported cause; use taid --help for command syntax. If another writer changed the project, inspect/reload it and reapply the intended edit; never retry a stale write blindly.".into(),
            "Stop if the intended project, source provenance or explicit review authorization is unclear.".into(),
        ],
    }
}

/// Run the standalone command without initializing a Tauri application or event loop.
pub fn run() -> i32 {
    let arguments = match Arguments::try_parse() {
        Ok(arguments) => arguments,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            print!("{error}");
            return 0;
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string(&error_envelope(error.to_string()))
                    .expect("JSON envelope is serializable")
            );
            return 1;
        }
    };
    match execute(arguments) {
        Ok(envelope) => {
            println!(
                "{}",
                serde_json::to_string(&envelope).expect("JSON envelope is serializable")
            );
            0
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string(&error_envelope(error))
                    .expect("JSON envelope is serializable")
            );
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_requires_explicit_design_review_and_operations_are_opt_in() {
        assert!(Arguments::try_parse_from([
            "taid",
            "--project",
            "project",
            "approve",
            "--ids",
            "asset"
        ])
        .is_err());
        assert!(Arguments::try_parse_from([
            "taid",
            "--project",
            "project",
            "approve",
            "--ids",
            "asset",
            "--qc-reviewed"
        ])
        .is_err());
        let arguments =
            Arguments::try_parse_from(["taid", "--project", "project", "process", "--ids", "a,b"])
                .unwrap();
        match arguments.command {
            Command::Process {
                ids,
                snap,
                alpha,
                palette,
                trim,
                scale,
            } => {
                assert_eq!(ids, ["a", "b"]);
                assert!(!snap && !alpha && !palette && !trim);
                assert_eq!(scale, 1);
            }
            _ => panic!("expected process command"),
        }
        let cutout =
            Arguments::try_parse_from(["taid", "--project", "project", "cutout", "--ids", "a,b"])
                .unwrap();
        assert!(matches!(cutout.command, Command::Cutout { ids } if ids == ["a", "b"]));
        assert!(Arguments::try_parse_from(["taid", "cutout"]).is_err());
        assert!(Arguments::try_parse_from(["taid", "settings", "set"]).is_err());
        assert!(
            Arguments::try_parse_from(["taid", "settings", "set", "--effort", "medium"]).is_ok()
        );
    }

    #[test]
    fn json_envelopes_keep_machine_readable_contract() {
        let envelope = success(
            "Inspected",
            serde_json::json!({ "id": "stable-id" }),
            vec!["local/path".into()],
            &["Inspect"],
        )
        .unwrap();
        let value = serde_json::to_value(envelope).unwrap();
        let mut keys: Vec<_> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["artifacts", "data", "nextActions", "status", "summary"]
        );
        assert_eq!(value["status"], "success");
        assert_eq!(value["data"]["id"], "stable-id");
        let error = serde_json::to_value(error_envelope("A reported cause".into())).unwrap();
        assert_eq!(error["status"], "error");
        assert!(error["data"].is_null());
        assert!(error["nextActions"].as_array().unwrap().len() >= 2);
    }

    fn command(arguments: &[&str]) -> Result<Envelope, String> {
        let arguments =
            Arguments::try_parse_from(std::iter::once("taid").chain(arguments.iter().copied()))
                .map_err(|error| error.to_string())?;
        execute(arguments)
    }

    #[test]
    fn cli_drives_shared_engine_and_retains_review_and_recent_boundaries() {
        static ENVIRONMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _environment = ENVIRONMENT.lock().unwrap();
        struct Fixture {
            root: PathBuf,
            previous_data_dir: Option<std::ffi::OsString>,
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                match &self.previous_data_dir {
                    Some(path) => std::env::set_var("TAID_DATA_DIR", path),
                    None => std::env::remove_var("TAID_DATA_DIR"),
                }
                let _ = fs::remove_dir_all(&self.root);
            }
        }
        let root = std::env::temp_dir().join(format!("taid-cli-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let fixture = Fixture {
            root,
            previous_data_dir: std::env::var_os("TAID_DATA_DIR"),
        };
        std::env::set_var("TAID_DATA_DIR", fixture.root.join("app-data"));
        let parent = fixture.root.to_str().unwrap();

        assert!(command(&["inspect"]).is_err());
        let saved = command(&[
            "settings",
            "set",
            "--model",
            "test-model",
            "--effort",
            "medium",
        ])
        .unwrap();
        assert_eq!(saved.data["model"], "test-model");
        command(&["settings", "set", "--effort", "low"]).unwrap();
        let settings = command(&["settings", "get"]).unwrap();
        assert_eq!(settings.data["model"], "test-model");
        assert_eq!(settings.data["effort"], "low");

        let created = command(&[
            "project",
            "create",
            "--parent",
            parent,
            "--name",
            "Synthetic",
        ])
        .unwrap();
        let project = created.data["root"].as_str().unwrap().to_owned();
        assert_eq!(
            command(&["--project", &project, "inspect"]).unwrap().data["assets"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        let profiles = command(&["--project", &project, "profile", "list"]).unwrap();
        let mut profile = profiles
            .data
            .as_array()
            .unwrap()
            .iter()
            .find(|profile| profile["id"] == "generic")
            .unwrap()
            .clone();
        profile["notes"] = Value::String("Synthetic fixture rules".into());
        let profile_path = fixture.root.join("profile.json");
        fs::write(&profile_path, serde_json::to_vec(&profile).unwrap()).unwrap();
        command(&[
            "--project",
            &project,
            "profile",
            "put",
            "--file",
            profile_path.to_str().unwrap(),
        ])
        .unwrap();

        let source = fixture.root.join("walk_00.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([40, 80, 120, 255]))
            .save(&source)
            .unwrap();
        let imported = command(&[
            "--project",
            &project,
            "import",
            source.to_str().unwrap(),
            "--profile",
            "generic",
        ])
        .unwrap();
        let asset = &imported.data["assets"][0];
        let id = asset["id"].as_str().unwrap().to_owned();
        let original_hash = asset["originalHash"].clone();
        assert_eq!(asset["sourceName"], "walk_00.png");
        assert_eq!(asset["sequence"], "walk");
        assert_eq!(asset["frameIndex"], 0);
        assert_eq!(asset["designApproved"], false);

        let patch = fixture.root.join("patch.json");
        fs::write(
            &patch,
            br#"{"reference":true,"notes":"Synthetic reference"}"#,
        )
        .unwrap();
        command(&[
            "--project",
            &project,
            "asset",
            "update",
            &id,
            "--file",
            patch.to_str().unwrap(),
        ])
        .unwrap();
        assert!(command(&[
            "--project",
            &project,
            "approve",
            "--ids",
            &id,
            "--design-reviewed",
            "--qc-reviewed"
        ])
        .is_err());
        fs::write(
            &patch,
            br#"{"reference":false,"notes":"Synthetic production fixture"}"#,
        )
        .unwrap();
        command(&[
            "--project",
            &project,
            "asset",
            "update",
            &id,
            "--file",
            patch.to_str().unwrap(),
        ])
        .unwrap();
        command(&[
            "--project",
            &project,
            "notes",
            "--text",
            "Source: synthetic test pixels",
        ])
        .unwrap();
        let processed = command(&[
            "--project",
            &project,
            "process",
            "--ids",
            &id,
            "--scale",
            "2",
        ])
        .unwrap();
        assert_eq!(processed.data["assets"][0]["width"], 8);
        assert_eq!(processed.data["assets"][0]["originalHash"], original_hash);
        assert_eq!(processed.data["assets"][0]["designApproved"], false);
        assert!(command(&[
            "--project",
            &project,
            "export",
            "--ids",
            &id,
            "--out",
            parent
        ])
        .is_err());
        let reviewed = command(&[
            "--project",
            &project,
            "approve",
            "--ids",
            &id,
            "--design-reviewed",
        ])
        .unwrap();
        assert_eq!(reviewed.data["assets"][0]["designApproved"], true);
        assert_eq!(reviewed.data["assets"][0]["qcApproved"], false);
        assert!(command(&[
            "--project",
            &project,
            "export",
            "--ids",
            &id,
            "--out",
            parent
        ])
        .is_err());
        command(&[
            "--project",
            &project,
            "approve",
            "--ids",
            &id,
            "--design-reviewed",
            "--qc-reviewed",
        ])
        .unwrap();
        let exported = command(&[
            "--project",
            &project,
            "export",
            "--ids",
            &id,
            "--out",
            parent,
        ])
        .unwrap();
        let delivery = Path::new(exported.data["path"].as_str().unwrap());
        assert!(delivery.join("walk_00.png").is_file());
        assert!(delivery.join("manifest.json").is_file());
        let reset = command(&["--project", &project, "reset", "--ids", &id]).unwrap();
        assert_eq!(reset.data["assets"][0]["width"], 4);
        assert_eq!(reset.data["assets"][0]["processed"], false);
        assert_eq!(reset.data["assets"][0]["qcApproved"], false);
        assert_eq!(reset.data["assets"][0]["originalHash"], original_hash);

        let other = command(&["project", "create", "--parent", parent, "--name", "Other"]).unwrap();
        let other_root = other.data["root"].as_str().unwrap();
        command(&["project", "open", &project]).unwrap();
        fs::remove_dir_all(other_root).unwrap();
        let recent = command(&["project", "recent"]).unwrap();
        let recent = recent.data.as_array().unwrap();
        assert_eq!(recent[0]["root"], project);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[1]["available"], false);
    }
}
