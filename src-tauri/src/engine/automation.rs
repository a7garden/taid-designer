use super::variants::{PreparedVariant, VariantSnapshot};
use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationItem {
    pub asset_id: String,
    pub name: String,
    pub steps: Vec<String>,
    pub warnings: Vec<String>,
    pub output_width: u32,
    pub output_height: u32,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationPlan {
    pub items: Vec<AutomationItem>,
    pub changed_count: usize,
    pub blocked: Vec<String>,
}

fn record_warnings(issues: &[Issue], stage: &str, warnings: &mut Vec<String>) {
    for issue in issues.iter().filter(|issue| issue.severity == "warning") {
        let warning = format!("{stage}: {}", issue.message);
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
}

fn canvases(asset: &Asset, profile: &RuleProfile) -> Result<((u32, u32), (u32, u32))> {
    if asset.reference {
        return Err("참조 이미지는 자동 준비하지 않습니다. 작업 에셋만 선택하세요.".into());
    }
    let grid = profile.pixel_size;
    if grid == 0 || grid > 64 {
        return Err("제작 기준 도트 크기는 1~64여야 합니다.".into());
    }
    let aligned = (
        asset.width.div_ceil(grid) * grid,
        asset.height.div_ceil(grid) * grid,
    );
    pixels::dimensions_valid(aligned.0, aligned.1)?;
    let target = (
        profile.width.unwrap_or(aligned.0),
        profile.height.unwrap_or(aligned.1),
    );
    pixels::dimensions_valid(target.0, target.1)?;
    if !target.0.is_multiple_of(grid) || !target.1.is_multiple_of(grid) {
        return Err("제작 기준의 고정 캔버스는 도트 크기의 배수여야 합니다.".into());
    }
    Ok((aligned, target))
}

// Planning simulates the same pipeline as committing, dropping each output
// immediately so a 128-item selection does not retain 128 decoded images.
fn prepare(snapshot: &VariantSnapshot, index: usize) -> Result<(AutomationItem, PreparedVariant)> {
    let asset = &snapshot.assets[index];
    let profile = snapshot.profile(index);
    let grid = profile.pixel_size;
    let (aligned, target) = canvases(asset, profile)?;
    let mut image = snapshot.load_image(index)?;
    let mut item = AutomationItem {
        asset_id: asset.id.clone(),
        name: asset.name.clone(),
        steps: Vec::new(),
        warnings: Vec::new(),
        output_width: target.0,
        output_height: target.1,
    };
    let mut pivot = (asset.pivot_x, asset.pivot_y);
    if image.dimensions() != aligned {
        let mut padded = RgbaImage::new(aligned.0, aligned.1);
        image::imageops::replace(&mut padded, &image, 0, 0);
        image = padded;
        item.steps
            .push("오른쪽·아래 투명 여백으로 격자 배수 맞춤".into());
        item.warnings.push("캔버스를 오른쪽·아래로 확장합니다. 좌상단과 피벗은 유지하지만 가장자리 블록의 그림은 다음 격자 정리에서 사라질 수 있습니다.".into());
    }
    let (mut issues, mut metrics) = pixels::inspect(&image, profile)?;
    if metrics.grid_violations > 0 {
        pixels::snap(&mut image, grid);
        item.steps.push("제작 기준 격자로 정리".into());
        item.warnings.push("격자 정리는 블록의 최빈 RGBA만 남깁니다. 세부 그림·색·투명도가 달라지거나 사라질 수 있으며 아트 노이즈 제거를 보장하지 않습니다.".into());
        (issues, metrics) = pixels::inspect(&image, profile)?;
        record_warnings(&issues, "격자 정리 후", &mut item.warnings);
    }
    if profile.alpha_mode == "binary" && metrics.semi_transparent > 0 {
        pixels::harden(&mut image, profile.alpha_threshold);
        item.steps
            .push(format!("알파 임계값 {}로 이진화", profile.alpha_threshold));
        item.warnings
            .push("알파 이진화는 임계값 아래의 그림을 지우고 나머지를 불투명하게 만듭니다.".into());
        issues = pixels::inspect(&image, profile)?.0;
        record_warnings(&issues, "알파 이진화 후", &mut item.warnings);
    }
    if issues.iter().any(|issue| issue.code == "palette") {
        let palette = pixels::palette(profile)?;
        pixels::map_palette(&mut image, &palette);
        item.steps.push("저장된 제작 기준 팔레트에 색 맞춤".into());
        item.warnings.push("팔레트 맞춤은 RGB 거리가 가장 가까운 저장 색으로 대체합니다. 색 차이와 의도한 세부가 합쳐질 수 있습니다.".into());
        issues = pixels::inspect(&image, profile)?.0;
        record_warnings(&issues, "팔레트 맞춤 후", &mut item.warnings);
    }
    if image.dimensions() != target {
        let settings = TileSettings {
            width: target.0 / grid,
            height: target.1 / grid,
            scale: grid,
            mode: TileMode::Fit,
            anchor: TileAnchor::Center,
        };
        (image, pivot) = tile::render(&image, grid, pivot, &settings)?;
        item.steps.push(format!(
            "전체 캔버스를 {}×{}px에 최근접 맞춤",
            target.0, target.1
        ));
        item.warnings.push("전체 캔버스를 중앙 맞춤합니다. 축소 시 논리 도트가 사라질 수 있고 정수 도트 반올림으로 비율과 피벗이 달라질 수 있습니다. 그림을 자르거나 여백을 자동 제거하지 않습니다.".into());
        issues = pixels::inspect(&image, profile)?.0;
        record_warnings(&issues, "캔버스 맞춤 후", &mut item.warnings);
    }
    record_warnings(&issues, "결과 검사", &mut item.warnings);
    let errors: Vec<_> = issues
        .iter()
        .filter(|issue| issue.severity == "error")
        .map(|issue| issue.message.as_str())
        .collect();
    if !errors.is_empty() {
        return Err(format!(
            "자동 준비 결과가 제작 기준을 통과하지 못했습니다: {}",
            errors.join(" ")
        ));
    }
    let provenance = serde_json::json!({"steps": item.steps, "warnings": item.warnings, "profileId": profile.id});
    Ok((
        item,
        PreparedVariant {
            image,
            profile: None,
            pivot,
            provenance,
        },
    ))
}

fn plan(snapshot: &VariantSnapshot) -> AutomationPlan {
    let mut plan = AutomationPlan::default();
    for index in 0..snapshot.assets.len() {
        match prepare(snapshot, index) {
            Ok((item, _)) => {
                plan.changed_count += usize::from(!item.steps.is_empty());
                plan.items.push(item);
            }
            Err(error) => plan
                .blocked
                .push(format!("{}: {error}", snapshot.assets[index].name)),
        }
    }
    if plan.changed_count > snapshot.remaining_capacity {
        plan.blocked
            .push("자동 준비 결과가 프로젝트 에셋 한도 2000개를 초과합니다.".into());
    }
    plan
}

impl EngineState {
    pub fn automation_plan(&self, ids: &[String]) -> Result<AutomationPlan> {
        match self.prepare_variants(ids) {
            Ok(snapshot) => Ok(plan(&snapshot)),
            Err(error) => Ok(AutomationPlan {
                blocked: vec![error],
                ..AutomationPlan::default()
            }),
        }
    }

    pub fn auto_prepare(&self, ids: &[String]) -> Result<Workspace> {
        let mut snapshot = self.prepare_variants(ids)?;
        let mut changed = Vec::with_capacity(snapshot.assets.len());
        let mut changed_count = 0;
        for (index, asset) in snapshot.assets.iter().enumerate() {
            let profile = snapshot.profile(index);
            let (aligned, target) =
                canvases(asset, profile).map_err(|error| format!("{}: {error}", asset.name))?;
            // Snapshot metrics were inspected for this exact current hash and
            // profile by the engine. Verify bytes even for no-op selections;
            // decoding and transforming again during preflight is unnecessary.
            if hash_file(Path::new(&asset.file_path))? != asset.current_hash {
                return Err(format!(
                    "{}: 변형 원본이 변경되었습니다. 프로젝트를 다시 불러오세요.",
                    asset.name
                ));
            }
            if asset.metrics.opaque + asset.metrics.semi_transparent == 0 {
                return Err(format!(
                    "{}: 보이는 그림이 없는 이미지는 자동 준비할 수 없습니다.",
                    asset.name
                ));
            }
            let needs_change = (asset.width, asset.height) != aligned
                || aligned != target
                || asset.metrics.grid_violations > 0
                || (profile.alpha_mode == "binary" && asset.metrics.semi_transparent > 0)
                || asset.issues.iter().any(|issue| issue.code == "palette");
            changed.push(needs_change);
            changed_count += usize::from(needs_change);
        }
        if changed_count > snapshot.remaining_capacity {
            return Err("자동 준비 결과가 프로젝트 에셋 한도 2000개를 초과합니다.".into());
        }
        if changed_count == 0 {
            return self
                .current_workspace()?
                .ok_or("먼저 작업 폴더를 여세요.".into());
        }
        snapshot.retain_changed(&changed);
        self.add_variants(&snapshot, "prepared", |index, _| {
            prepare(&snapshot, index).map(|(_, prepared)| prepared)
        })
    }
}
