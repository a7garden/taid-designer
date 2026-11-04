use super::*;

/// Manifest and current-image identities captured before an operation starts.
pub(crate) struct VariantSnapshot {
    root: PathBuf,
    revision: Option<String>,
    pub assets: Vec<Asset>,
    profiles: Vec<RuleProfile>,
    pub(super) remaining_capacity: usize,
}

pub(super) struct PreparedVariant {
    pub image: RgbaImage,
    pub profile: Option<RuleProfile>,
    pub pivot: (f64, f64),
    pub provenance: serde_json::Value,
}

pub(super) fn bounded_label(value: &str, max_bytes: usize) -> &str {
    let mut end = value.len().min(max_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

impl VariantSnapshot {
    pub fn load_image(&self, index: usize) -> Result<RgbaImage> {
        let asset = self
            .assets
            .get(index)
            .ok_or("변형 원본 선택이 올바르지 않습니다.")?;
        let path = Path::new(&asset.file_path);
        if hash_file(path)? != asset.current_hash {
            return Err("변형 원본이 변경되었습니다. 프로젝트를 다시 불러오세요.".into());
        }
        let image = pixels::load(path)?;
        if hash_file(path)? != asset.current_hash
            || image.dimensions() != (asset.width, asset.height)
        {
            return Err("변형 원본이 읽는 도중 변경되었습니다. 다시 불러오세요.".into());
        }
        Ok(image)
    }

    pub(super) fn profile(&self, index: usize) -> &RuleProfile {
        &self.profiles[index]
    }

    pub(super) fn retain_changed(&mut self, changed: &[bool]) {
        let mut index = 0;
        self.assets.retain(|_| {
            let keep = changed[index];
            index += 1;
            keep
        });
        let mut index = 0;
        self.profiles.retain(|_| {
            let keep = changed[index];
            index += 1;
            keep
        });
    }
}

impl EngineState {
    pub(crate) fn prepare_variants(&self, ids: &[String]) -> Result<VariantSnapshot> {
        let state = self.lock()?;
        let project = state.project.as_ref().ok_or("먼저 작업 폴더를 여세요.")?;
        let _lock = WorkspaceLock::acquire(&project.root)?;
        project.check_revision()?;
        let selected = project.selection(ids)?;
        if selected.len() > 128 {
            return Err("변형은 한 번에 128개까지, 프로젝트는 2000개까지 지원합니다.".into());
        }
        let profiles = selected
            .iter()
            .map(|&index| {
                project
                    .profile(&project.document.assets[index].asset.profile_id)
                    .cloned()
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(VariantSnapshot {
            root: project.root.clone(),
            revision: project.baseline_sha.clone(),
            remaining_capacity: MAX_ASSETS.saturating_sub(project.document.assets.len()),
            profiles,
            assets: selected
                .into_iter()
                .map(|index| {
                    let mut asset = project.document.assets[index].asset.clone();
                    asset.original_path = project
                        .root
                        .join(&asset.original_path)
                        .to_string_lossy()
                        .into_owned();
                    asset.file_path = project
                        .root
                        .join(&asset.file_path)
                        .to_string_lossy()
                        .into_owned();
                    asset
                })
                .collect(),
        })
    }

    /// A single manifest replacement commits the entire batch. Only newly created
    /// files are removed on failure, and each result becomes its own reset base.
    pub(super) fn add_variants(
        &self,
        snapshot: &VariantSnapshot,
        suffix: &str,
        mut prepare: impl FnMut(usize, &Asset) -> Result<PreparedVariant>,
    ) -> Result<Workspace> {
        if snapshot.assets.is_empty() {
            return Err("변형할 에셋을 선택하세요.".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.project.clone().ok_or("먼저 작업 폴더를 여세요.")?;
        if candidate.root != snapshot.root || candidate.baseline_sha != snapshot.revision {
            return Err(
                "변형 작업 중 프로젝트가 변경되었습니다. 결과를 저장하지 않았습니다.".into(),
            );
        }
        let _lock = WorkspaceLock::acquire(&candidate.root)?;
        candidate.check_revision()?;
        candidate.refresh()?;
        if candidate.document.assets.len() + snapshot.assets.len() > MAX_ASSETS {
            return Err("프로젝트 에셋 한도 2000개를 초과합니다.".into());
        }
        let mut names: HashSet<String> = candidate
            .document
            .assets
            .iter()
            .map(|stored| stored.asset.name.to_ascii_lowercase())
            .collect();
        let mut sequences: HashSet<String> = candidate
            .document
            .assets
            .iter()
            .map(|stored| stored.asset.sequence.clone())
            .collect();
        let mut derivative_sequences = HashMap::<String, String>::new();
        let mut created = Vec::<PathBuf>::new();
        let outcome = (|| {
            for (index, expected) in snapshot.assets.iter().enumerate() {
                let parent = candidate.document.assets[candidate.index(&expected.id)?]
                    .asset
                    .clone();
                if parent.current_hash != expected.current_hash
                    || parent.original_hash != expected.original_hash
                    || (parent.width, parent.height) != (expected.width, expected.height)
                {
                    return Err(
                        "변형 원본의 해시 또는 캔버스가 변경되었습니다. 저장하지 않았습니다."
                            .into(),
                    );
                }
                let prepared = prepare(index, &parent)?;
                pixels::dimensions_valid(prepared.image.width(), prepared.image.height())?;
                let mut profile_id = parent.profile_id.clone();
                if let Some(mut profile) = prepared.profile {
                    validate_profile(&mut profile)?;
                    profile_id = profile.id.clone();
                    if let Some(existing) = candidate
                        .document
                        .profiles
                        .iter()
                        .find(|p| p.id == profile.id)
                    {
                        if existing != &profile {
                            return Err("변형 제작 기준 식별자가 충돌합니다.".into());
                        }
                    } else {
                        if candidate.document.profiles.len() >= 100 {
                            return Err("제작 기준은 최대 100개까지 지원합니다.".into());
                        }
                        candidate.document.profiles.push(profile);
                    }
                }
                let name = unique_name(&stem(&format!("{}_{suffix}", parent.name)), &names);
                names.insert(name.to_ascii_lowercase());
                let sequence_key = if parent.sequence.is_empty() {
                    format!("asset:{}", parent.id)
                } else {
                    format!("sequence:{}", parent.sequence)
                };
                let sequence = derivative_sequences
                    .entry(sequence_key)
                    .or_insert_with(|| {
                        let source = if parent.sequence.is_empty() {
                            &parent.name
                        } else {
                            &parent.sequence
                        };
                        let base = format!("{}_{suffix}", bounded_label(source, 180));
                        let mut value = base.clone();
                        let mut number = 2;
                        while sequences.contains(&value) {
                            value = format!("{base}_{number}");
                            number += 1;
                        }
                        sequences.insert(value.clone());
                        value
                    })
                    .clone();
                let id = Uuid::new_v4().to_string();
                let (original_path, original_hash) =
                    save_png_new(&candidate.root, "originals", &id, &prepared.image)?;
                created.push(candidate.root.join(&original_path));
                let provenance = serde_json::json!({
                    "operation": suffix,
                    "parentId": parent.id,
                    "sourceHash": parent.original_hash,
                    "inputHash": parent.current_hash,
                    "settings": prepared.provenance,
                });
                let notes = format!(
                    "{}{}Taid provenance: {}",
                    parent.notes,
                    if parent.notes.is_empty() { "" } else { "\n" },
                    provenance
                );
                let mut asset = parent;
                asset.parent_id = Some(asset.id.clone());
                asset.parent_hash = Some(asset.current_hash.clone());
                asset.id = id;
                asset.name = name;
                asset.source_name = format!("{}.png", asset.name);
                asset.original_path = original_path.clone();
                asset.file_path = original_path;
                asset.original_hash = original_hash.clone();
                asset.current_hash = original_hash;
                asset.profile_id = profile_id;
                asset.width = prepared.image.width();
                asset.height = prepared.image.height();
                asset.pivot_x = prepared.pivot.0;
                asset.pivot_y = prepared.pivot.1;
                asset.sequence = sequence;
                asset.notes = notes;
                asset.crop_x = 0;
                asset.crop_y = 0;
                asset.processed = false;
                asset.design_approved = false;
                asset.qc_approved = false;
                asset.issues.clear();
                asset.metrics = Metrics::default();
                validate_metadata(&asset)?;
                candidate.document.assets.push(StoredAsset {
                    source_pivot_x: asset.pivot_x,
                    source_pivot_y: asset.pivot_y,
                    asset,
                    transform: Transform::default(),
                    design_proof: None,
                    qc_proof: None,
                });
            }
            candidate.refresh()?;
            for expected in &snapshot.assets {
                let parent = &candidate.document.assets[candidate.index(&expected.id)?].asset;
                if parent.current_hash != expected.current_hash
                    || parent.original_hash != expected.original_hash
                {
                    return Err(
                        "저장 직전에 변형 원본이 변경되었습니다. 결과를 저장하지 않았습니다."
                            .into(),
                    );
                }
            }
            candidate.persist_locked()?;
            Ok(())
        })();
        if let Err(error) = outcome {
            for path in created {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
        let view = candidate.view();
        state.project = Some(candidate);
        Ok(view)
    }

    pub(crate) fn add_cutout_variants(
        &self,
        snapshot: &VariantSnapshot,
        staged_pngs: &[PathBuf],
        model: &str,
        model_revision: &str,
    ) -> Result<Workspace> {
        if staged_pngs.len() != snapshot.assets.len() || staged_pngs.is_empty() {
            return Err("누끼 결과 수가 선택한 원본 수와 다릅니다.".into());
        }
        text(model, 200, "누끼 모델")?;
        text(model_revision, 200, "누끼 모델 버전")?;
        self.add_variants(snapshot, "cutout", |index, parent| {
            let image = pixels::load(&staged_pngs[index])?;
            if image.dimensions() != (parent.width, parent.height) {
                return Err("누끼 결과는 원본과 동일한 전체 캔버스여야 합니다.".into());
            }
            let source = snapshot.load_image(index)?;
            if source.pixels().zip(image.pixels()).any(|(before, after)| {
                before.0[..3] != after.0[..3] || after[3] > before[3]
            }) {
                return Err("누끼 결과는 RGB를 변경하거나 기존 투명도를 불투명하게 만들 수 없습니다.".into());
            }
            Ok(PreparedVariant {
                image, profile: None, pivot: (parent.pivot_x, parent.pivot_y),
                provenance: serde_json::json!({"operation": "local-background-removal", "model": model, "modelRevision": model_revision}),
            })
        })
    }
}
