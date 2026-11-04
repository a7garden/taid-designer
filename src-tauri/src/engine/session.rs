use super::*;

const RECENTS_FILE: &str = "recent-projects.json";
const MAX_RECENTS: usize = 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecentProject {
    pub root: String,
    pub name: String,
    pub last_opened: String,
    pub available: bool,
}

fn recent_directory() -> Result<PathBuf> {
    let directory = crate::settings::data_dir()?;
    fs::create_dir_all(&directory).map_err(|e| err("앱 데이터 폴더를 만들 수 없습니다", e))?;
    Ok(directory)
}

fn load_recents(directory: &Path) -> Result<Vec<RecentProject>> {
    let path = directory.join(RECENTS_FILE);
    let mut projects: Vec<RecentProject> = if path.exists() {
        serde_json::from_slice(&read_json_file(&path, 256 * 1024)?)
            .map_err(|e| err("최근 프로젝트 기록이 올바르지 않습니다", e))?
    } else {
        let legacy = directory.join("recent-workspace.json");
        let projects = if legacy.exists() {
            let root: String = serde_json::from_slice(&read_json_file(&legacy, 32768)?)
                .map_err(|e| err("이전 작업 폴더 기록이 올바르지 않습니다", e))?;
            let name = read_json_file(&Path::new(&root).join(PROJECT_FILE), MAX_JSON_BYTES)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|value| {
                    value
                        .get("name")
                        .and_then(|name| name.as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| {
                    Path::new(&root)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                });
            vec![RecentProject {
                root,
                name,
                last_opened: Utc::now().to_rfc3339(),
                available: false,
            }]
        } else {
            Vec::new()
        };
        // The new file is the migration marker; the old selection is never rewritten or removed.
        atomic_write(&path, &json_bytes(&projects)?)?;
        projects
    };
    if projects.len() > MAX_RECENTS {
        return Err("최근 프로젝트 기록은 최대 20개여야 합니다.".into());
    }
    let mut roots = HashSet::new();
    for project in &mut projects {
        text(&project.root, 32768, "프로젝트 경로")?;
        text(&project.name, 200, "프로젝트 이름")?;
        if !Path::new(&project.root).is_absolute() || !roots.insert(project.root.clone()) {
            return Err("최근 프로젝트 경로가 올바르지 않거나 중복되었습니다.".into());
        }
        project.available = Path::new(&project.root).join(PROJECT_FILE).is_file();
    }
    Ok(projects)
}

fn lock_recents(directory: &Path) -> Result<File> {
    fs::create_dir_all(directory)
        .map_err(|error| err("최근 프로젝트 폴더를 만들 수 없습니다", error))?;
    let path = directory.join(".recents.lock");
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("최근 프로젝트 잠금 파일은 심볼릭 링크일 수 없습니다.".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| err("최근 프로젝트 잠금 파일을 열 수 없습니다", error))?;
    file.lock_exclusive()
        .map_err(|error| err("최근 프로젝트 기록을 잠글 수 없습니다", error))?;
    Ok(file)
}

fn recent_projects_in(directory: &Path) -> Result<Vec<RecentProject>> {
    let _lock = lock_recents(directory)?;
    load_recents(directory)
}

fn record_recent_in(directory: &Path, root: &str, name: &str) -> Result<()> {
    let _lock = lock_recents(directory)?;
    let mut projects = load_recents(directory)?;
    projects.retain(|project| project.root != root);
    projects.insert(
        0,
        RecentProject {
            root: root.into(),
            name: name.into(),
            last_opened: Utc::now().to_rfc3339(),
            available: true,
        },
    );
    projects.truncate(MAX_RECENTS);
    atomic_write(&directory.join(RECENTS_FILE), &json_bytes(&projects)?)
}

pub fn recent_projects() -> Result<Vec<RecentProject>> {
    recent_projects_in(&recent_directory()?)
}

impl EngineState {
    pub(super) fn record_recent(&self, root: &str, name: &str) -> Result<()> {
        let directory = self
            .recent_directory
            .clone()
            .map(Ok)
            .unwrap_or_else(recent_directory)?;
        record_recent_in(&directory, root, name)
    }

    pub(super) fn activate_project(&self, project: Project) -> Result<Workspace> {
        let mut state = self.lock()?;
        let workspace = project.view();
        self.record_recent(&workspace.root, &workspace.name)?;
        state.project = Some(project);
        Ok(workspace)
    }

    pub fn create_project(&self, parent: &Path, name: &str) -> Result<Workspace> {
        self.activate_project(Project::create(parent, name)?)
    }

    pub fn open_project(&self, path: &Path) -> Result<Workspace> {
        self.activate_project(Project::open(path)?)
    }

    /// Returns the loaded snapshot without replacing it with external edits.
    pub fn current_workspace(&self) -> Result<Option<Workspace>> {
        Ok(self.lock()?.project.as_ref().map(Project::view))
    }

    pub fn workspace_changed(&self) -> Result<bool> {
        Ok(self
            .lock()?
            .project
            .as_ref()
            .is_some_and(|project| project.check_revision().is_err()))
    }

    pub fn reload_workspace(&self) -> Result<Workspace> {
        let mut state = self.lock()?;
        let root = &state
            .project
            .as_ref()
            .ok_or("먼저 작업 폴더를 여세요")?
            .root;
        let project = Project::open(root)?;
        let workspace = project.view();
        self.record_recent(&workspace.root, &workspace.name)?;
        state.project = Some(project);
        Ok(workspace)
    }

    pub fn import_assets(&self, paths: &[String], profile_id: &str) -> Result<Workspace> {
        self.mutate(|project| project.import(paths, profile_id))
    }

    pub fn save_profile(&self, mut profile: RuleProfile) -> Result<Workspace> {
        validate_profile(&mut profile)?;
        self.mutate(|project| {
            if let Some(index) = project
                .document
                .profiles
                .iter()
                .position(|old| old.id == profile.id)
            {
                if project.document.profiles[index] != profile {
                    for asset in &mut project.document.assets {
                        if asset.asset.profile_id == profile.id {
                            asset.clear_all();
                        }
                    }
                    project.document.profiles[index] = profile;
                }
            } else {
                project.document.profiles.push(profile);
            }
            Ok(())
        })
    }

    pub fn save_notes(&self, notes: String) -> Result<Workspace> {
        text(&notes, 100000, "프로젝트 메모")?;
        self.mutate(|project| {
            if project.document.notes != notes {
                project.document.notes = notes;
                for asset in &mut project.document.assets {
                    asset.clear_qc();
                }
            }
            Ok(())
        })
    }

    pub fn update_asset(&self, id: &str, patch: AssetPatch) -> Result<Workspace> {
        self.mutate(|project| project.update(id, patch))
    }

    pub fn process_assets(&self, ids: &[String], settings: &ProcessSettings) -> Result<Workspace> {
        self.mutate(|project| project.process(ids, settings))
    }

    pub fn reset_assets(&self, ids: &[String]) -> Result<Workspace> {
        self.mutate(|project| {
            project.selection(ids)?;
            for id in ids {
                project.reset(id)?;
            }
            Ok(())
        })
    }

    pub fn approve_assets(&self, ids: &[String], design: bool, qc: bool) -> Result<Workspace> {
        self.mutate(|project| {
            project.refresh()?;
            project.selection(ids)?;
            for id in ids {
                project.approve(id, design, qc)?;
            }
            Ok(())
        })
    }

    pub fn export_assets(&self, parent: &Path, ids: &[String]) -> Result<ExportResult> {
        let mut state = self.lock()?;
        let mut project = state.project.clone().ok_or("먼저 작업 폴더를 여세요")?;
        let _lock = WorkspaceLock::acquire(&project.root)?;
        project.check_revision()?;
        project.refresh()?;
        self.record_recent(&project.root.to_string_lossy(), &project.document.name)?;
        project.persist_locked()?;
        let result = project.export(parent, ids);
        state.project = Some(project);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_selection_migrates_once_and_keeps_unreachable_entries() {
        let root = std::env::temp_dir().join(format!("taid-recents-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let first = Project::create(&root, "First").unwrap();
        let second = Project::create(&root, "Second").unwrap();
        let store = root.join("app-data");
        fs::create_dir(&store).unwrap();
        atomic_write(
            &store.join("recent-workspace.json"),
            &json_bytes(&first.root.to_string_lossy()).unwrap(),
        )
        .unwrap();
        let migrated = recent_projects_in(&store).unwrap();
        assert_eq!(migrated[0].root, first.root.to_string_lossy());
        record_recent_in(&store, &second.root.to_string_lossy(), "Second").unwrap();
        atomic_write(
            &store.join("recent-workspace.json"),
            &json_bytes(&first.root.to_string_lossy()).unwrap(),
        )
        .unwrap();
        fs::remove_dir_all(&first.root).unwrap();
        let projects = recent_projects_in(&store).unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].root, second.root.to_string_lossy());
        assert!(projects[0].available);
        assert!(!projects[1].available);
        for index in 0..25 {
            record_recent_in(
                &store,
                &root.join(format!("missing-{index}")).to_string_lossy(),
                "Missing",
            )
            .unwrap();
        }
        assert_eq!(recent_projects_in(&store).unwrap().len(), MAX_RECENTS);
    }
}

#[cfg(test)]
mod concurrent_tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    #[test]
    fn unrelated_project_writers_preserve_all_recent_selections() {
        struct Store(PathBuf);
        impl Drop for Store {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let store =
            Store(std::env::temp_dir().join(format!("taid-recent-writers-{}", Uuid::new_v4())));
        fs::create_dir(&store.0).unwrap();
        let gate = Arc::new(Barrier::new(8));
        let mut writers = Vec::new();
        for index in 0..8 {
            let directory = store.0.clone();
            let gate = gate.clone();
            writers.push(std::thread::spawn(move || {
                let root = directory
                    .join(format!("project-{index}"))
                    .to_string_lossy()
                    .into_owned();
                gate.wait();
                record_recent_in(&directory, &root, &format!("Project {index}")).unwrap();
                root
            }));
        }
        let expected: HashSet<_> = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect();
        let actual: HashSet<_> = recent_projects_in(&store.0)
            .unwrap()
            .into_iter()
            .map(|project| project.root)
            .collect();
        assert_eq!(actual, expected);
    }
}
