use super::*;
use serde_json::json;

pub(super) fn spreadsheet_text(value: &str) -> String {
    let trimmed = value.trim_start_matches([' ', '\t', '\r', '\n']);
    let control_prefix = value
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '\t' | '\r' | '\n'));
    if control_prefix
        || trimmed
            .chars()
            .next()
            .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@'))
    {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

impl Project {
    pub(super) fn export(&self, destination: &Path, ids: &[String]) -> Result<ExportResult> {
        let indices = self.selection(ids)?;
        let mut names = HashSet::new();
        for &index in &indices {
            let stored = &self.document.assets[index];
            let a = &stored.asset;
            let profile = self.profile(&a.profile_id)?;
            if a.reference
                || !a.design_approved
                || !a.qc_approved
                || a.issues.iter().any(|i| i.severity == "error")
                || stored.design_proof.as_deref()
                    != Some(stored.design_fingerprint(profile)?.as_str())
                || stored.qc_proof.as_deref()
                    != Some(
                        stored
                            .qc_fingerprint(profile, &self.document.notes)?
                            .as_str(),
                    )
            {
                return Err(format!("'{}'은 참고용이거나 승인되지 않았거나 QC 오류가 있습니다. 변경 후 재승인이 필요한 경우도 포함됩니다. 아무 파일도 내보내지 않았습니다.",a.name));
            }
            valid_name(&a.name)?;
            if !names.insert(a.name.to_ascii_lowercase()) {
                return Err("선택한 전달 파일명이 중복됩니다.".into());
            }
        }
        let destination =
            fs::canonicalize(destination).map_err(|e| err("기존 내보내기 폴더를 선택하세요", e))?;
        if !destination.is_dir() {
            return Err("내보내기 대상은 폴더여야 합니다.".into());
        }
        let folder_name = format!(
            "Taid_{}_{}",
            Utc::now().format("%Y%m%d_%H%M%S"),
            Uuid::new_v4().simple()
        );
        let staging = destination.join(format!(".{folder_name}.partial"));
        let final_path = destination.join(&folder_name);
        fs::create_dir(&staging).map_err(|e| err("새 전달 폴더를 만들 수 없습니다", e))?;
        let outcome = (|| -> Result<()> {
            let mut manifest_assets = Vec::with_capacity(indices.len());
            let mut csv = csv::Writer::from_writer(Vec::<u8>::new());
            csv.write_record([
                "filename",
                "profile_id",
                "stage",
                "kind",
                "sequence",
                "frame_index",
                "duration_ms",
                "width_px",
                "height_px",
                "pivot_x_px",
                "pivot_y_px",
                "pivot_origin",
                "crop_x_px",
                "crop_y_px",
                "scale",
                "layer",
                "notes",
                "source_name",
                "original_sha256",
                "sha256",
            ])
            .map_err(|e| err("CSV 헤더를 기록할 수 없습니다", e))?;
            for &index in &indices {
                let stored = &self.document.assets[index];
                let a = &stored.asset;
                let profile = self.profile(&a.profile_id)?;
                let filename = format!("{}.png", a.name);
                let source = confined(&self.root, &a.file_path)?;
                if hash_file(&source)? != a.current_hash {
                    return Err(format!(
                        "내보내는 중 '{}'이 변경되었습니다. 다시 검수하고 승인하세요.",
                        a.name
                    ));
                }
                let target = staging.join(&filename);
                let mut input =
                    File::open(&source).map_err(|e| err("전달할 PNG를 열 수 없습니다", e))?;
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .map_err(|e| err("전달 PNG를 만들 수 없습니다", e))?;
                std::io::copy(&mut input, &mut output)
                    .map_err(|e| err("전달 PNG를 복사할 수 없습니다", e))?;
                output
                    .sync_all()
                    .map_err(|e| err("전달 PNG를 디스크에 동기화할 수 없습니다", e))?;
                if hash_file(&target)? != a.current_hash {
                    return Err("복사 중 PNG가 변경되어 미완성 전달 폴더를 폐기했습니다.".into());
                }
                let original = pixels::load(&confined(&self.root, &a.original_path)?)?;
                manifest_assets.push(json!({
                    "id": a.id, "filename": filename, "profileId": a.profile_id,
                    "sequence": a.sequence, "frameIndex": a.frame_index, "durationMs": a.duration_ms,
                    "width": a.width, "height": a.height,
                    "pivot": { "x": a.pivot_x, "y": a.pivot_y, "origin": "top-left", "units": "physical-pixels", "axes": "x-right,y-down" },
                    "cropOffset": { "x": a.crop_x, "y": a.crop_y, "units": "scaled-original-physical-pixels" },
                    "originalCanvas": { "width": original.width(), "height": original.height() },
                    "sourcePivot": { "x": stored.source_pivot_x, "y": stored.source_pivot_y, "units": "original-physical-pixels" },
                    "processing": stored.transform, "layer": a.layer, "notes": a.notes,
                    "source": { "filename": a.source_name, "originalSha256": a.original_hash },
                    "sha256": a.current_hash, "designApproved": true, "qcApproved": true,
                }));
                csv.write_record(vec![
                    spreadsheet_text(&filename),
                    spreadsheet_text(&a.profile_id),
                    spreadsheet_text(&profile.stage),
                    spreadsheet_text(&profile.kind),
                    spreadsheet_text(&a.sequence),
                    a.frame_index.to_string(),
                    a.duration_ms.to_string(),
                    a.width.to_string(),
                    a.height.to_string(),
                    a.pivot_x.to_string(),
                    a.pivot_y.to_string(),
                    "top-left pixels; x-right y-down".into(),
                    a.crop_x.to_string(),
                    a.crop_y.to_string(),
                    stored.transform.scale.to_string(),
                    a.layer.to_string(),
                    spreadsheet_text(&a.notes),
                    spreadsheet_text(&a.source_name),
                    a.original_hash.clone(),
                    a.current_hash.clone(),
                ])
                .map_err(|e| err("CSV 에셋 정보를 기록할 수 없습니다", e))?;
            }
            let used: HashSet<_> = indices
                .iter()
                .map(|i| self.document.assets[*i].asset.profile_id.as_str())
                .collect();
            let profiles: Vec<_> = self
                .document
                .profiles
                .iter()
                .filter(|p| used.contains(p.id.as_str()))
                .collect();
            let manifest = json!({
                "schemaVersion": SCHEMA, "application": "Taid", "exportedAt": Utc::now().to_rfc3339(),
                "project": { "name": self.document.name, "notes": self.document.notes },
                "coordinateSystem": { "origin": "top-left", "units": "physical-pixels", "axes": "x-right,y-down" },
                "processingOrder": ["immutable original", "optional alpha threshold", "optional nearest explicit palette RGB", "optional origin-aligned block majority RGBA", "optional common animation union trim", "optional integer nearest-neighbor scale"],
                "profiles": profiles, "assets": manifest_assets,
                "developerBoundary": "Individual PNG delivery only. Developer creates atlas and Unity import settings. No .meta or automatic Unity changes.",
            });
            atomic_write(&staging.join("manifest.json"), &json_bytes(&manifest)?)?;
            let bytes = csv
                .into_inner()
                .map_err(|e| err("CSV 기록을 완료할 수 없습니다", e))?;
            atomic_write(&staging.join("assets.csv"), &bytes)?;
            let mut handoff = format!("# {} — PNG handoff\n\n{} approved individual PNG files. Frame order, durations, pivots, layers, notes and hashes are in manifest.json and assets.csv.\n\n## Coordinates and animation\n\nPivots use physical pixels from the top-left (x right, y down), not normalized Unity coordinates. Convert them deliberately at import. Bottom-center is the original default. Crop offsets are measured in scaled-original physical pixels; sourcePivot and processing retain original-space metadata. Each selected animation sequence was union-trimmed only when all its frames were selected. Intentional movement is preserved; frames are never auto-centered.\n\nProcessing always starts from immutable project originals: optional alpha threshold → explicit nearest RGB palette → fixed-origin block-majority snap (ties: first row-major pixel) → common sequence union trim → integer nearest-neighbor scale. The per-file processing list records which operations actually ran. No fixed 16-color palette is assumed.\n\n## Developer boundary\n\nThe developer creates sprite atlases and decides final Unity texture import/compression settings. This package contains no atlas, .meta files, or automatic Unity project edits. Review pixel sharpness and animation alignment in the target game.\n\nMovement, rotation, scale, opacity and lighting can be engine-only effects. Asset and project notes below are the delivery instructions; do not bake them into PNGs unless explicitly requested.\n\n## Project notes\n\n{}\n\n## Asset notes\n", self.document.name,indices.len(),if self.document.notes.trim().is_empty() { "No project-level engine-effect instructions supplied." } else { &self.document.notes });
            for &index in &indices {
                let a = &self.document.assets[index].asset;
                handoff.push_str(&format!(
                    "\n### {}.png\n\nLayer: {}. Sequence: {}. Frame: {}. Duration: {} ms.\n\n{}\n",
                    a.name,
                    a.layer,
                    if a.sequence.is_empty() {
                        "(none)"
                    } else {
                        &a.sequence
                    },
                    a.frame_index,
                    a.duration_ms,
                    if a.notes.trim().is_empty() {
                        "No asset-specific engine-effect instructions supplied."
                    } else {
                        &a.notes
                    }
                ));
            }
            handoff.push_str("\n## CSV text safety\n\nText cells that could be interpreted as spreadsheet formulas are prefixed with an apostrophe in assets.csv. Read manifest.json for exact original notes, sequence names and source filenames.\n");
            atomic_write(&staging.join("HANDOFF.md"), handoff.as_bytes())?;
            if final_path.exists() {
                return Err("전달 폴더명이 충돌했습니다. 기존 대상은 덮어쓰지 않았습니다.".into());
            }
            fs::rename(&staging, &final_path)
                .map_err(|e| err("전달 폴더 생성을 완료할 수 없습니다", e))?;
            Ok(())
        })();
        if outcome.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        outcome?;
        Ok(ExportResult {
            path: final_path.to_string_lossy().into_owned(),
            count: indices.len(),
        })
    }
}
