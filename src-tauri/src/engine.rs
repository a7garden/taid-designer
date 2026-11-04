use chrono::Utc;
use fs2::FileExt;
use image::{ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
use tauri::Manager;
use uuid::Uuid;

mod delivery;
mod session;
pub use session::{recent_projects, RecentProject};
mod automation;
mod palette;
mod pixels;
mod tile;
mod variants;
pub use automation::{AutomationItem, AutomationPlan};
pub use palette::{read_palette_file, PaletteColor, PaletteOutside, PaletteReport};
pub use tile::{TileAnchor, TileMode, TileSettings};
#[cfg(test)]
mod tests;

type Result<T, E = String> = std::result::Result<T, E>;
const SCHEMA: u32 = 1;
const PROJECT_FILE: &str = "taid-project.json";
const MAX_ASSETS: usize = 2000;
const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleProfile {
    pub id: String,
    pub name: String,
    pub stage: String,
    pub kind: String,
    pub pixel_size: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub alpha_mode: String,
    pub alpha_threshold: u8,
    pub palette: Vec<String>,
    pub notes: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub pixels: u64,
}
impl Issue {
    fn error(code: &str, message: &str, pixels: u64) -> Self {
        Self {
            code: code.into(),
            severity: "error".into(),
            message: message.into(),
            pixels,
        }
    }
    fn warning(code: &str, message: &str, pixels: u64) -> Self {
        Self {
            code: code.into(),
            severity: "warning".into(),
            message: message.into(),
            pixels,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Metrics {
    pub colors: u64,
    pub semi_transparent: u64,
    pub opaque: u64,
    pub grid_violations: u64,
    pub near_color_variants: u64,
    pub isolated_color_dots: u64,
    pub visible_logical_dots: u64,
    pub orphan_check_applied: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub source_name: String,
    pub width: u32,
    pub height: u32,
    pub original_path: String,
    pub file_path: String,
    pub original_hash: String,
    pub current_hash: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub parent_hash: Option<String>,
    pub profile_id: String,
    pub sequence: String,
    pub frame_index: u32,
    pub duration_ms: u32,
    pub pivot_x: f64,
    pub pivot_y: f64,
    pub layer: i32,
    pub notes: String,
    pub reference: bool,
    pub crop_x: u32,
    pub crop_y: u32,
    pub processed: bool,
    pub design_approved: bool,
    pub qc_approved: bool,
    pub issues: Vec<Issue>,
    pub metrics: Metrics,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub schema_version: u32,
    pub name: String,
    pub root: String,
    pub notes: String,
    pub profiles: Vec<RuleProfile>,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetPatch {
    pub name: Option<String>,
    pub profile_id: Option<String>,
    pub sequence: Option<String>,
    pub frame_index: Option<u32>,
    pub duration_ms: Option<u32>,
    pub pivot_x: Option<f64>,
    pub pivot_y: Option<f64>,
    pub layer: Option<i32>,
    pub notes: Option<String>,
    pub reference: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProcessSettings {
    pub snap_to_grid: bool,
    pub harden_alpha: bool,
    pub map_palette: bool,
    pub trim: bool,
    pub scale: u32,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Transform {
    scale: u32,
    crop_x: u32,
    crop_y: u32,
    operations: Vec<String>,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            scale: 1,
            crop_x: 0,
            crop_y: 0,
            operations: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAsset {
    #[serde(flatten)]
    asset: Asset,
    source_pivot_x: f64,
    source_pivot_y: f64,
    transform: Transform,
    #[serde(default)]
    design_proof: Option<String>,
    #[serde(default)]
    qc_proof: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    schema_version: u32,
    name: String,
    notes: String,
    profiles: Vec<RuleProfile>,
    assets: Vec<StoredAsset>,
}
#[derive(Clone)]
struct Project {
    root: PathBuf,
    document: Document,
    inspections: HashMap<String, Inspection>,
    baseline_sha: Option<String>,
}
#[derive(Clone)]
struct Inspection {
    image_hash: String,
    rule_hash: String,
    width: u32,
    height: u32,
    issues: Vec<Issue>,
    metrics: Metrics,
}
#[derive(Default)]
struct State {
    project: Option<Project>,
}
#[derive(Default)]
pub struct EngineState {
    inner: Mutex<State>,
    recent_directory: Option<PathBuf>,
}

struct WorkspaceLock {
    _file: File,
}
impl WorkspaceLock {
    fn acquire(root: &Path) -> Result<Self> {
        let path = root.join(".taid.lock");
        if path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err("프로젝트 잠금 파일은 심볼릭 링크일 수 없습니다.".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| err("프로젝트 잠금 파일을 열 수 없습니다", e))?;
        file.try_lock_exclusive().map_err(|e| {
            err(
                "다른 창이나 CLI가 프로젝트를 사용 중입니다. 작업이 끝난 뒤 다시 실행하세요",
                e,
            )
        })?;
        Ok(Self { _file: file })
    }
}
fn read_json_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| err("JSON 파일을 읽을 수 없습니다", e))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return Err("JSON 파일은 허용된 크기 이하의 일반 파일이어야 합니다.".into());
    }
    fs::read(path).map_err(|e| err("JSON 파일을 읽을 수 없습니다", e))
}
fn inferred_frame(source_stem: &str) -> (String, u32) {
    if let Some((prefix, number)) = source_stem.rsplit_once(['_', '-']) {
        if !prefix.is_empty() && number.len() >= 2 && number.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(frame) = number.parse::<u32>() {
                return (stem(prefix), frame);
            }
        }
    }
    (stem(source_stem), 0)
}

fn err(context: &str, error: impl std::fmt::Display) -> String {
    format!("{context}: {error}")
}
fn text(value: &str, max: usize, field: &str) -> Result<()> {
    if value.len() > max || value.contains('\0') {
        return Err(format!("{field}이 너무 길거나 null 문자를 포함합니다."));
    }
    Ok(())
}
fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn valid_name(value: &str) -> Result<()> {
    if !safe_id(value) {
        return Err("파일명은 확장자 없이 영문·숫자·밑줄·하이픈 1~100자로 입력하세요.".into());
    }
    let upper = value.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
    {
        return Err("Windows에서 예약된 파일명입니다. 다른 이름을 입력하세요.".into());
    }
    Ok(())
}
fn stem(value: &str) -> String {
    let mut result: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    result = result.trim_matches('_').to_owned();
    if valid_name(&result).is_err() {
        "asset".into()
    } else {
        result
    }
}
fn unique_name(base: &str, names: &HashSet<String>) -> String {
    if !names.contains(&base.to_ascii_lowercase()) {
        return base.into();
    }
    let mut number = 2;
    loop {
        let candidate = format!("{base}_{number}");
        if !names.contains(&candidate.to_ascii_lowercase()) {
            return candidate;
        }
        number += 1;
    }
}
fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_file(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| err("이미지 해시를 계산할 수 없습니다", e))?;
    if file
        .metadata()
        .map_err(|e| err("이미지 파일 정보를 읽을 수 없습니다", e))?
        .len()
        > pixels::MAX_FILE_BYTES
    {
        return Err("이미지가 64 MiB를 초과합니다.".into());
    }
    let mut reader = std::io::BufReader::new(file);
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|e| err("이미지 해시를 계산할 수 없습니다", e))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|e| err("JSON을 기록할 수 없습니다", e))
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("상위 폴더가 없습니다")?;
    let temporary = parent.join(format!(".taid-{}.tmp", Uuid::new_v4()));
    let outcome = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| err("임시 파일을 만들 수 없습니다", e))?;
        file.write_all(bytes)
            .map_err(|e| err("프로젝트를 기록할 수 없습니다", e))?;
        file.sync_all()
            .map_err(|e| err("프로젝트 기록을 디스크에 동기화할 수 없습니다", e))?;
        fs::rename(&temporary, path).map_err(|e| err("프로젝트 저장을 완료할 수 없습니다", e))?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(temporary);
    }
    outcome
}
fn confined(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("프로젝트에 안전하지 않은 이미지 경로가 있습니다.".into());
    }
    let resolved = fs::canonicalize(root.join(path))
        .map_err(|e| err("프로젝트 이미지 파일을 찾을 수 없습니다", e))?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err("이미지 경로가 선택한 작업 폴더 밖을 가리킵니다.".into());
    }
    let mut signature = [0u8; 8];
    File::open(&resolved)
        .and_then(|mut file| file.read_exact(&mut signature))
        .map_err(|e| err("PNG 파일을 읽을 수 없습니다", e))?;
    if signature != [137, 80, 78, 71, 13, 10, 26, 10] {
        return Err("작업 폴더의 이미지는 실제 PNG 파일이어야 합니다. JPEG 원본은 가져오기로 다시 등록하세요.".into());
    }
    Ok(resolved)
}
fn ensure_directories(root: &Path) -> Result<()> {
    for name in ["originals", "processed"] {
        let path = root.join(name);
        if !path.exists() {
            fs::create_dir(&path).map_err(|e| err("이미지 폴더를 만들 수 없습니다", e))?;
        }
        if fs::symlink_metadata(&path)
            .map_err(|e| err("이미지 폴더 정보를 읽을 수 없습니다", e))?
            .file_type()
            .is_symlink()
            || !path.is_dir()
        {
            return Err(
                "이미지 폴더는 심볼릭 링크가 아닌 작업 폴더 안의 실제 폴더여야 합니다.".into(),
            );
        }
    }
    Ok(())
}
fn save_png_new(
    root: &Path,
    directory: &str,
    id: &str,
    image: &RgbaImage,
) -> Result<(String, String)> {
    if !safe_id(id) || !matches!(directory, "originals" | "processed") {
        return Err("안전하지 않은 이미지 식별자입니다.".into());
    }
    ensure_directories(root)?;
    let relative = format!("{directory}/{id}.png");
    let path = root.join(&relative);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| err("PNG 파일을 만들 수 없습니다", e))?;
    let outcome = (|| {
        let mut writer = std::io::BufWriter::new(file);
        image
            .write_to(&mut writer, ImageFormat::Png)
            .map_err(|e| err("PNG를 인코딩할 수 없습니다", e))?;
        writer
            .flush()
            .map_err(|e| err("PNG 기록을 완료할 수 없습니다", e))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|e| err("PNG 기록을 디스크에 동기화할 수 없습니다", e))
    })();
    if let Err(error) = outcome {
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok((relative, hash_file(&path)?))
}
fn validate_profile(profile: &mut RuleProfile) -> Result<()> {
    if !safe_id(&profile.id) {
        return Err("프로필 ID는 영문·숫자·밑줄·하이픈으로 입력하세요.".into());
    }
    text(&profile.name, 200, "프로필 이름")?;
    text(&profile.stage, 200, "스테이지")?;
    text(&profile.notes, 20000, "프로필 메모")?;
    if profile.name.trim().is_empty()
        || !matches!(
            profile.kind.as_str(),
            "sprite" | "background" | "ui" | "effect"
        )
    {
        return Err("프로필 이름과 지원하는 에셋 종류를 지정하세요.".into());
    }
    if !(1..=64).contains(&profile.pixel_size) {
        return Err("픽셀 블록 크기는 1~64의 정수여야 합니다.".into());
    }
    for dimension in [profile.width, profile.height].into_iter().flatten() {
        if dimension == 0 || dimension > pixels::MAX_DIMENSION {
            return Err("캔버스 크기는 1~16384px로 지정하거나 비워 두세요.".into());
        }
    }
    if let (Some(w), Some(h)) = (profile.width, profile.height) {
        pixels::dimensions_valid(w, h)?;
    }
    if !matches!(profile.alpha_mode.as_str(), "binary" | "preserve") || profile.alpha_threshold == 0
    {
        return Err("알파 보존/이진화를 선택하고 임계값을 1~255로 입력하세요.".into());
    }
    let colors = pixels::palette(profile)?;
    let mut seen = HashSet::new();
    profile.palette = colors
        .into_iter()
        .filter(|c| seen.insert(*c))
        .map(|c| format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]))
        .collect();
    if profile.palette.len() > 256 {
        return Err("프로필 팔레트는 중복을 제외하고 최대 256색까지 지정할 수 있습니다.".into());
    }
    Ok(())
}
fn default_profiles() -> Vec<RuleProfile> {
    let make = |id: &str,
                name: &str,
                stage: &str,
                kind: &str,
                pixel_size,
                alpha_mode: &str,
                width,
                height,
                notes: &str| RuleProfile {
        id: id.into(),
        name: name.into(),
        stage: stage.into(),
        kind: kind.into(),
        pixel_size,
        width,
        height,
        alpha_mode: alpha_mode.into(),
        alpha_threshold: 128,
        palette: Vec::new(),
        notes: notes.into(),
    };
    vec![
        make(
            "generic",
            "일반 · 원본 보존",
            "",
            "sprite",
            1,
            "preserve",
            None,
            None,
            "스테이지별 캔버스와 팔레트를 임의로 지정하지 않습니다.",
        ),
        make(
            "stage03-sprite",
            "Stage 03 · 4px 스프라이트",
            "Stage03",
            "sprite",
            4,
            "binary",
            None,
            None,
            "Stage 03 전용입니다. 필요하면 에셋에 맞는 캔버스를 지정하세요.",
        ),
        make(
            "stage03-background",
            "Stage 03 · 1672 × 944 배경",
            "Stage03",
            "background",
            4,
            "binary",
            Some(1672),
            Some(944),
            "Stage 03 배경 전용입니다. 다른 에셋에 일괄 적용하지 않습니다.",
        ),
        make(
            "effect",
            "이펙트 · 반투명 보존",
            "",
            "effect",
            1,
            "preserve",
            None,
            None,
            "의도한 글로우와 반투명을 보존합니다. 엔진에서 처리할 연출은 메모에 적으세요.",
        ),
    ]
}

impl StoredAsset {
    fn clear_all(&mut self) {
        self.asset.design_approved = false;
        self.asset.qc_approved = false;
        self.design_proof = None;
        self.qc_proof = None;
    }
    fn clear_qc(&mut self) {
        self.asset.qc_approved = false;
        self.qc_proof = None;
    }
    fn design_fingerprint(&self, profile: &RuleProfile) -> Result<String> {
        Ok(hash_bytes(
            &serde_json::to_vec(&(&self.asset.current_hash, &self.asset.original_hash, profile))
                .map_err(|e| err("디자인 승인 정보를 읽을 수 없습니다", e))?,
        ))
    }
    fn qc_fingerprint(&self, profile: &RuleProfile, project_notes: &str) -> Result<String> {
        let a = &self.asset;
        Ok(hash_bytes(
            &serde_json::to_vec(&(
                self.design_fingerprint(profile)?,
                &a.name,
                &a.sequence,
                a.frame_index,
                a.duration_ms,
                a.pivot_x,
                a.pivot_y,
                a.layer,
                &a.notes,
                a.reference,
                a.crop_x,
                a.crop_y,
                &self.transform,
                project_notes,
            ))
            .map_err(|e| err("QC 승인 정보를 읽을 수 없습니다", e))?,
        ))
    }
}

impl Project {
    fn view(&self) -> Workspace {
        let assets = self
            .document
            .assets
            .iter()
            .map(|stored| {
                let mut asset = stored.asset.clone();
                asset.original_path = self
                    .root
                    .join(&asset.original_path)
                    .to_string_lossy()
                    .into_owned();
                asset.file_path = self
                    .root
                    .join(&asset.file_path)
                    .to_string_lossy()
                    .into_owned();
                asset
            })
            .collect();
        Workspace {
            schema_version: SCHEMA,
            name: self.document.name.clone(),
            root: self.root.to_string_lossy().into_owned(),
            notes: self.document.notes.clone(),
            profiles: self.document.profiles.clone(),
            assets,
        }
    }
    fn persist(&mut self) -> Result<()> {
        let _lock = WorkspaceLock::acquire(&self.root)?;
        self.persist_locked()
    }
    fn check_revision(&self) -> Result<()> {
        let path = self.root.join(PROJECT_FILE);
        let current = if path.exists() {
            Some(hash_bytes(&read_json_file(&path, MAX_JSON_BYTES)?))
        } else {
            None
        };
        if current != self.baseline_sha {
            return Err("프로젝트가 다른 창이나 CLI에서 변경되었습니다. 저장하지 않았습니다. 작업 폴더를 다시 불러온 뒤 변경을 적용하세요 (reload required).".into());
        }
        Ok(())
    }
    fn persist_locked(&mut self) -> Result<()> {
        self.check_revision()?;
        let bytes = json_bytes(&self.document)?;
        if bytes.len() as u64 > MAX_JSON_BYTES {
            return Err(
                "프로젝트 메타데이터가 16 MiB를 초과합니다. 메모를 줄인 뒤 저장하세요.".into(),
            );
        }
        atomic_write(&self.root.join(PROJECT_FILE), &bytes)?;
        self.baseline_sha = Some(hash_bytes(&bytes));
        Ok(())
    }
    fn profile(&self, id: &str) -> Result<&RuleProfile> {
        self.document
            .profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("프로필을 찾을 수 없습니다: {id}"))
    }
    fn index(&self, id: &str) -> Result<usize> {
        if !safe_id(id) {
            return Err("안전하지 않은 에셋 식별자입니다.".into());
        }
        self.document
            .assets
            .iter()
            .position(|a| a.asset.id == id)
            .ok_or_else(|| format!("에셋을 찾을 수 없습니다: {id}"))
    }
    fn selection(&self, ids: &[String]) -> Result<Vec<usize>> {
        if ids.is_empty() {
            return Err("에셋을 하나 이상 선택하세요.".into());
        }
        let mut seen = HashSet::new();
        ids.iter()
            .map(|id| {
                if !seen.insert(id) {
                    return Err("선택 목록에 같은 에셋이 중복되어 있습니다.".into());
                }
                self.index(id)
            })
            .collect()
    }
    fn validate_structure(&mut self) -> Result<()> {
        if self.document.schema_version != SCHEMA {
            return Err("지원하지 않는 프로젝트 스키마 버전입니다.".into());
        }
        text(&self.document.name, 200, "프로젝트 이름")?;
        text(&self.document.notes, 100000, "프로젝트 메모")?;
        if self.document.name.trim().is_empty()
            || self.document.profiles.is_empty()
            || self.document.profiles.len() > 100
            || self.document.assets.len() > MAX_ASSETS
        {
            return Err("프로젝트 이름, 프로필 수 또는 에셋 수가 올바르지 않습니다.".into());
        }
        let mut profile_ids = HashSet::new();
        for profile in &mut self.document.profiles {
            validate_profile(profile)?;
            if !profile_ids.insert(profile.id.clone()) {
                return Err("프로필 식별자가 중복되어 있습니다.".into());
            }
        }
        let mut ids = HashSet::new();
        for stored in &self.document.assets {
            let a = &stored.asset;
            if !safe_id(&a.id) || !ids.insert(a.id.clone()) {
                return Err("에셋 식별자가 중복되었거나 안전하지 않습니다.".into());
            }
            valid_name(&a.name)?;
            if !profile_ids.contains(&a.profile_id) {
                return Err("에셋에 지정된 규칙 프로필이 없습니다.".into());
            }
            if a.original_path != format!("originals/{}.png", a.id)
                || !a.file_path.starts_with("processed/") && a.file_path != a.original_path
            {
                return Err("이미지가 허용된 프로젝트 경로에 있지 않습니다.".into());
            }
            if let Some(file) = a.file_path.strip_prefix("processed/") {
                if !file.ends_with(".png") || !safe_id(file.trim_end_matches(".png")) {
                    return Err("정제 이미지의 파일명이 안전하지 않습니다.".into());
                }
            }
            if !(1..=8).contains(&stored.transform.scale)
                || !stored.source_pivot_x.is_finite()
                || !stored.source_pivot_y.is_finite()
            {
                return Err("이미지 변환 메타데이터가 올바르지 않습니다.".into());
            }
            validate_metadata(a)?;
        }
        Ok(())
    }
    fn refresh(&mut self) -> Result<()> {
        self.validate_structure()?;
        let mut name_counts = HashMap::<String, usize>::new();
        let mut frame_counts = HashMap::<(String, u32), usize>::new();
        for stored in &self.document.assets {
            if stored.asset.reference {
                continue;
            }
            *name_counts
                .entry(stored.asset.name.to_ascii_lowercase())
                .or_default() += 1;
            if !stored.asset.sequence.is_empty() {
                *frame_counts
                    .entry((stored.asset.sequence.clone(), stored.asset.frame_index))
                    .or_default() += 1;
            }
        }
        let mut profiles = HashMap::new();
        for profile in &self.document.profiles {
            let rule_hash = hash_bytes(
                &serde_json::to_vec(profile).map_err(|e| err("규칙을 읽을 수 없습니다", e))?,
            );
            profiles.insert(profile.id.clone(), (profile.clone(), rule_hash));
        }
        for stored in &mut self.document.assets {
            let original = confined(&self.root, &stored.asset.original_path)?;
            let original_hash = hash_file(&original)?;
            if original_hash != stored.asset.original_hash {
                return Err(format!(
                    "'{}'의 보존 원본이 변경되었습니다. 원본 파일을 복구한 뒤 프로젝트를 여세요.",
                    stored.asset.name
                ));
            }
            let current = confined(&self.root, &stored.asset.file_path)?;
            let hash = if current == original {
                original_hash
            } else {
                hash_file(&current)?
            };
            let (profile, rule_hash) = profiles
                .get(&stored.asset.profile_id)
                .ok_or("규칙 프로필이 없습니다")?;
            let inspection = if let Some(cached) = self
                .inspections
                .get(&stored.asset.id)
                .filter(|cached| cached.image_hash == hash && cached.rule_hash == *rule_hash)
            {
                cached.clone()
            } else {
                let image = pixels::load(&current)?;
                let (issues, metrics) = pixels::inspect(&image, profile)?;
                let inspection = Inspection {
                    image_hash: hash.clone(),
                    rule_hash: rule_hash.clone(),
                    width: image.width(),
                    height: image.height(),
                    issues,
                    metrics,
                };
                self.inspections
                    .insert(stored.asset.id.clone(), inspection.clone());
                inspection
            };
            let changed = hash != stored.asset.current_hash
                || inspection.width != stored.asset.width
                || inspection.height != stored.asset.height;
            if changed {
                stored.clear_all();
                stored.asset.current_hash = hash;
                stored.asset.width = inspection.width;
                stored.asset.height = inspection.height;
            }
            let mut issues = inspection.issues;
            let metrics = inspection.metrics;
            if !stored.asset.reference
                && name_counts
                    .get(&stored.asset.name.to_ascii_lowercase())
                    .copied()
                    .unwrap_or(0)
                    > 1
            {
                issues.push(Issue::error(
                    "duplicate-name",
                    "전달 파일명은 대소문자를 구분하지 않고 고유해야 합니다.",
                    0,
                ));
            }
            if !stored.asset.reference
                && !stored.asset.sequence.is_empty()
                && frame_counts
                    .get(&(stored.asset.sequence.clone(), stored.asset.frame_index))
                    .copied()
                    .unwrap_or(0)
                    > 1
            {
                issues.push(Issue::error(
                    "duplicate-frame",
                    "같은 애니메이션 시퀀스 안에서 프레임 번호가 중복됩니다.",
                    0,
                ));
            }
            if changed {
                issues.push(Issue::warning("artifact-changed", "Taid 밖에서 PNG가 변경되어 두 승인을 해제했습니다. 변경된 그림을 다시 검수하세요.", 0));
            }
            stored.asset.issues = issues;
            stored.asset.metrics = metrics;
            if stored.design_proof.as_deref() != Some(stored.design_fingerprint(profile)?.as_str())
            {
                stored.asset.design_approved = false;
                stored.design_proof = None;
            }
            if !stored.asset.design_approved
                || stored.qc_proof.as_deref()
                    != Some(
                        stored
                            .qc_fingerprint(profile, &self.document.notes)?
                            .as_str(),
                    )
                || stored.asset.issues.iter().any(|i| i.severity == "error")
            {
                stored.clear_qc();
            }
        }
        Ok(())
    }
    fn create(parent: &Path, name: &str) -> Result<Self> {
        text(name, 200, "프로젝트 이름")?;
        if name.trim().is_empty() {
            return Err("프로젝트 이름을 입력하세요.".into());
        }
        let parent = fs::canonicalize(parent).map_err(|e| err("기존 상위 폴더를 선택하세요", e))?;
        if !parent.is_dir() {
            return Err("프로젝트의 상위 경로는 폴더여야 합니다.".into());
        }
        let base = stem(name);
        let mut root = parent.join(&base);
        while root.exists() {
            root = parent.join(format!(
                "{base}-{}",
                &Uuid::new_v4().simple().to_string()[..8]
            ));
        }
        fs::create_dir(&root).map_err(|e| err("작업 폴더를 만들 수 없습니다", e))?;
        ensure_directories(&root)?;
        let mut project = Self {
            root,
            document: Document {
                schema_version: SCHEMA,
                name: name.trim().into(),
                notes: String::new(),
                profiles: default_profiles(),
                assets: Vec::new(),
            },
            inspections: HashMap::new(),
            baseline_sha: None,
        };
        project.persist()?;
        Ok(project)
    }
    fn open(path: &Path) -> Result<Self> {
        let path = if path.is_file() {
            if path.file_name().and_then(|p| p.to_str()) != Some(PROJECT_FILE) {
                return Err("Taid 작업 폴더 또는 taid-project.json을 선택하세요.".into());
            }
            path.parent().ok_or("프로젝트 폴더가 없습니다")?
        } else {
            path
        };
        let root = fs::canonicalize(path).map_err(|e| err("작업 폴더를 열 수 없습니다", e))?;
        let _lock = WorkspaceLock::acquire(&root)?;
        let manifest = root.join(PROJECT_FILE);
        let metadata = fs::symlink_metadata(&manifest)
            .map_err(|e| err("프로젝트 파일을 읽을 수 없습니다", e))?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_JSON_BYTES
        {
            return Err("프로젝트 파일은 16 MiB 이하의 일반 파일이어야 합니다.".into());
        }
        let bytes = fs::read(&manifest).map_err(|e| err("프로젝트 파일을 읽을 수 없습니다", e))?;
        let document: Document = serde_json::from_slice(&bytes)
            .map_err(|e| err("프로젝트 JSON이 올바르지 않습니다", e))?;
        let mut project = Self {
            root,
            document,
            inspections: HashMap::new(),
            baseline_sha: Some(hash_bytes(&bytes)),
        };
        ensure_directories(&project.root)?;
        project.refresh()?;
        Ok(project)
    }
    fn import(&mut self, paths: &[String], profile_id: &str) -> Result<()> {
        self.profile(profile_id)?;
        if paths.is_empty()
            || paths.len() > 128
            || self.document.assets.len() + paths.len() > MAX_ASSETS
        {
            return Err(
                "한 번에 1~128개 이미지를 가져올 수 있습니다. 프로젝트 한도는 2000개입니다.".into(),
            );
        }
        let mut names: HashSet<String> = self
            .document
            .assets
            .iter()
            .map(|a| a.asset.name.to_ascii_lowercase())
            .collect();
        for path in paths {
            let source = Path::new(path);
            let image = pixels::load(source)?;
            let source_name = source
                .file_name()
                .and_then(|p| p.to_str())
                .ok_or("이미지 파일명이 올바른 UTF-8이 아닙니다")?
                .to_owned();
            let source_stem = source
                .file_stem()
                .and_then(|p| p.to_str())
                .unwrap_or("asset");
            let base = stem(source_stem);
            let name = unique_name(&base, &names);
            names.insert(name.to_ascii_lowercase());
            let (sequence, frame_index) = inferred_frame(source_stem);
            let id = Uuid::new_v4().to_string();
            let (original_path, original_hash) =
                save_png_new(&self.root, "originals", &id, &image)?;
            let pivot_x = f64::from(image.width()) / 2.0;
            let pivot_y = f64::from(image.height());
            self.document.assets.push(StoredAsset {
                asset: Asset {
                    id,
                    name,
                    source_name,
                    width: image.width(),
                    height: image.height(),
                    file_path: original_path.clone(),
                    original_path,
                    current_hash: original_hash.clone(),
                    original_hash,
                    parent_id: None,
                    parent_hash: None,
                    profile_id: profile_id.into(),
                    sequence,
                    frame_index,
                    duration_ms: 100,
                    pivot_x,
                    pivot_y,
                    layer: 0,
                    notes: String::new(),
                    reference: false,
                    crop_x: 0,
                    crop_y: 0,
                    processed: false,
                    design_approved: false,
                    qc_approved: false,
                    issues: Vec::new(),
                    metrics: Metrics::default(),
                },
                source_pivot_x: pivot_x,
                source_pivot_y: pivot_y,
                transform: Transform::default(),
                design_proof: None,
                qc_proof: None,
            });
        }
        Ok(())
    }
    fn update(&mut self, id: &str, patch: AssetPatch) -> Result<()> {
        let index = self.index(id)?;
        if let Some(profile) = &patch.profile_id {
            self.profile(profile)?;
        }
        if let Some(name) = &patch.name {
            valid_name(name)?;
            if self
                .document
                .assets
                .iter()
                .enumerate()
                .any(|(i, a)| i != index && a.asset.name.eq_ignore_ascii_case(name))
            {
                return Err("다른 에셋이 이미 사용하는 파일명입니다.".into());
            }
        }
        let stored = &mut self.document.assets[index];
        let before = serde_json::to_vec(&stored.asset)
            .map_err(|e| err("에셋 정보를 읽을 수 없습니다", e))?;
        let old_profile = stored.asset.profile_id.clone();
        if let Some(v) = patch.name {
            stored.asset.name = v;
        }
        if let Some(v) = patch.profile_id {
            stored.asset.profile_id = v;
        }
        if let Some(v) = patch.sequence {
            stored.asset.sequence = v.trim().into();
        }
        if let Some(v) = patch.frame_index {
            stored.asset.frame_index = v;
        }
        if let Some(v) = patch.duration_ms {
            stored.asset.duration_ms = v;
        }
        if let Some(v) = patch.pivot_x {
            stored.asset.pivot_x = v;
            stored.source_pivot_x =
                v / f64::from(stored.transform.scale) + f64::from(stored.transform.crop_x);
        }
        if let Some(v) = patch.pivot_y {
            stored.asset.pivot_y = v;
            stored.source_pivot_y =
                v / f64::from(stored.transform.scale) + f64::from(stored.transform.crop_y);
        }
        if let Some(v) = patch.layer {
            stored.asset.layer = v;
        }
        if let Some(v) = patch.notes {
            stored.asset.notes = v;
        }
        if let Some(v) = patch.reference {
            stored.asset.reference = v;
        }
        validate_metadata(&stored.asset)?;
        if before
            != serde_json::to_vec(&stored.asset)
                .map_err(|e| err("에셋 정보를 읽을 수 없습니다", e))?
        {
            stored.clear_qc();
        }
        if old_profile != stored.asset.profile_id {
            stored.clear_all();
        }
        Ok(())
    }
    fn process(&mut self, ids: &[String], settings: &ProcessSettings) -> Result<()> {
        if !(1..=8).contains(&settings.scale) {
            return Err("정수 확대 배율은 1~8이어야 합니다.".into());
        }
        let selected = self.selection(ids)?;
        if settings.trim {
            let selected_ids: HashSet<_> = ids.iter().map(String::as_str).collect();
            for &index in &selected {
                let a = &self.document.assets[index].asset;
                if !a.sequence.is_empty()
                    && self.document.assets.iter().any(|other| {
                        other.asset.sequence == a.sequence
                            && !selected_ids.contains(other.asset.id.as_str())
                    })
                {
                    return Err(format!(
                        "공통 영역을 자르려면 '{}' 시퀀스의 모든 프레임을 선택하세요.",
                        a.sequence
                    ));
                }
            }
        }
        let mut prepared = Vec::<(usize, RgbaImage)>::new();
        let mut total_pixels = 0u64;
        for &index in &selected {
            let stored = &self.document.assets[index];
            let original = confined(&self.root, &stored.asset.original_path)?;
            if hash_file(&original)? != stored.asset.original_hash {
                return Err("보존 원본이 변경되어 정제를 중단했습니다.".into());
            }
            let profile = self.profile(&stored.asset.profile_id)?;
            let mut image = pixels::load(&original)?;
            total_pixels += u64::from(image.width()) * u64::from(image.height());
            if total_pixels > 64 * 1024 * 1024 {
                return Err(
                    "선택한 원본 이미지 합계가 일괄 정제 한도인 64메가픽셀을 초과합니다.".into(),
                );
            }
            if settings.harden_alpha {
                pixels::harden(&mut image, profile.alpha_threshold);
            }
            if settings.map_palette {
                let colors = pixels::palette(profile)?;
                if colors.is_empty() {
                    return Err(format!(
                        "'{}' 프로필에 팔레트를 먼저 지정하세요. 임의의 색상으로 바꾸지 않습니다.",
                        profile.name
                    ));
                }
                pixels::map_palette(&mut image, &colors);
            }
            if settings.snap_to_grid {
                pixels::snap(&mut image, profile.pixel_size);
            }
            prepared.push((index, image));
        }
        let mut unions = HashMap::<String, (u32, u32, pixels::Bounds)>::new();
        if settings.trim {
            for (index, image) in &prepared {
                let sequence = &self.document.assets[*index].asset.sequence;
                let key = if sequence.is_empty() {
                    format!("asset:{}", self.document.assets[*index].asset.id)
                } else {
                    format!("sequence:{sequence}")
                };
                let b = pixels::bounds(image).ok_or(
                    "보이는 그림이 없는 이미지는 자를 수 없습니다. 그림을 먼저 복구하세요.",
                )?;
                if let Some((width, height, union)) = unions.get_mut(&key) {
                    if *width != image.width() || *height != image.height() {
                        return Err(
                            "공통 영역을 자르려면 시퀀스의 원본 캔버스 크기가 모두 같아야 합니다."
                                .into(),
                        );
                    }
                    union.x = union.x.min(b.x);
                    union.y = union.y.min(b.y);
                    union.right = union.right.max(b.right);
                    union.bottom = union.bottom.max(b.bottom);
                } else {
                    unions.insert(key, (image.width(), image.height(), b));
                }
            }
        }
        for (index, mut image) in prepared {
            let stored = &mut self.document.assets[index];
            let mut transform = Transform {
                scale: settings.scale,
                ..Transform::default()
            };
            if settings.harden_alpha {
                transform.operations.push("alpha-threshold".into());
            }
            if settings.map_palette {
                transform
                    .operations
                    .push("nearest-explicit-palette-rgb".into());
            }
            if settings.snap_to_grid {
                transform
                    .operations
                    .push("origin-aligned-block-majority-rgba; ties=first-row-major".into());
            }
            if settings.trim {
                let key = if stored.asset.sequence.is_empty() {
                    format!("asset:{}", stored.asset.id)
                } else {
                    format!("sequence:{}", stored.asset.sequence)
                };
                let bounds = unions.get(&key).ok_or("자를 영역 정보가 없습니다")?.2;
                transform.crop_x = bounds.x;
                transform.crop_y = bounds.y;
                image = pixels::crop(&image, bounds);
                transform
                    .operations
                    .push("common-sequence-union-trim".into());
            }
            if settings.scale > 1 {
                image = pixels::scale(&image, settings.scale)?;
                transform
                    .operations
                    .push("integer-nearest-neighbor-scale".into());
            }
            let file_id = format!("{}-{}", stored.asset.id, Uuid::new_v4());
            let (path, hash) = save_png_new(&self.root, "processed", &file_id, &image)?;
            stored.asset.file_path = path;
            stored.asset.current_hash = hash;
            stored.asset.width = image.width();
            stored.asset.height = image.height();
            stored.asset.crop_x = transform.crop_x * settings.scale;
            stored.asset.crop_y = transform.crop_y * settings.scale;
            stored.asset.pivot_x =
                (stored.source_pivot_x - f64::from(transform.crop_x)) * f64::from(settings.scale);
            stored.asset.pivot_y =
                (stored.source_pivot_y - f64::from(transform.crop_y)) * f64::from(settings.scale);
            stored.asset.processed = true;
            stored.transform = transform;
            stored.clear_all();
        }
        Ok(())
    }
    fn reset(&mut self, id: &str) -> Result<()> {
        let index = self.index(id)?;
        let stored = &mut self.document.assets[index];
        let original = confined(&self.root, &stored.asset.original_path)?;
        let image = pixels::load(&original)?;
        if hash_file(&original)? != stored.asset.original_hash {
            return Err("보존 원본이 변경되어 초기화를 중단했습니다.".into());
        }
        stored.asset.file_path = stored.asset.original_path.clone();
        stored.asset.current_hash = stored.asset.original_hash.clone();
        stored.asset.width = image.width();
        stored.asset.height = image.height();
        stored.asset.pivot_x = stored.source_pivot_x;
        stored.asset.pivot_y = stored.source_pivot_y;
        stored.asset.crop_x = 0;
        stored.asset.crop_y = 0;
        stored.asset.processed = false;
        stored.transform = Transform::default();
        stored.clear_all();
        Ok(())
    }
    fn approve(&mut self, id: &str, design: bool, qc: bool) -> Result<()> {
        let index = self.index(id)?;
        let profile = self
            .profile(&self.document.assets[index].asset.profile_id)?
            .clone();
        let stored = &mut self.document.assets[index];
        if qc
            && (!design
                || stored.asset.reference
                || stored.asset.issues.iter().any(|i| i.severity == "error"))
        {
            return Err(
                "QC 승인은 디자인 승인 후, 참고용이 아닌 에셋에 QC 오류가 없을 때 가능합니다."
                    .into(),
            );
        }
        stored.asset.design_approved = design;
        stored.asset.qc_approved = qc;
        stored.design_proof = if design {
            Some(stored.design_fingerprint(&profile)?)
        } else {
            None
        };
        stored.qc_proof = if qc {
            Some(stored.qc_fingerprint(&profile, &self.document.notes)?)
        } else {
            None
        };
        Ok(())
    }
}

fn validate_metadata(asset: &Asset) -> Result<()> {
    text(&asset.sequence, 200, "시퀀스")?;
    text(&asset.notes, 20000, "에셋 메모")?;
    text(&asset.source_name, 1000, "원본 파일명")?;
    if !(1..=600000).contains(&asset.duration_ms) || asset.frame_index > 1000000 {
        return Err("표시 시간은 1~600000ms, 프레임 번호는 0~1000000이어야 합니다.".into());
    }
    if !asset.pivot_x.is_finite()
        || !asset.pivot_y.is_finite()
        || asset.pivot_x.abs() > 1000000.0
        || asset.pivot_y.abs() > 1000000.0
    {
        return Err("피벗은 좌상단 기준 픽셀 좌표로, ±1000000 이내의 유한한 수여야 합니다.".into());
    }
    pixels::dimensions_valid(asset.width, asset.height)
}

impl EngineState {
    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.inner
            .lock()
            .map_err(|_| "작업 상태를 사용할 수 없습니다. Taid를 다시 시작하세요.".into())
    }
    fn mutate(&self, action: impl FnOnce(&mut Project) -> Result<()>) -> Result<Workspace> {
        let mut state = self.lock()?;
        let mut candidate = state
            .project
            .clone()
            .ok_or("먼저 작업 폴더를 만들거나 여세요")?;
        let _lock = WorkspaceLock::acquire(&candidate.root)?;
        candidate.check_revision()?;
        self.record_recent(&candidate.root.to_string_lossy(), &candidate.document.name)?;
        action(&mut candidate)?;
        candidate.refresh()?;
        candidate.persist_locked()?;
        let view = candidate.view();
        state.project = Some(candidate);
        Ok(view)
    }
    pub fn validate_image_paths(&self, paths: &[String]) -> Result<Vec<String>> {
        let state = self.lock()?;
        let project = state
            .project
            .as_ref()
            .ok_or("이미지를 첨부하기 전에 작업 폴더를 여세요")?;
        if paths.len() > 32 {
            return Err("작업 폴더의 이미지는 최대 32개까지 첨부할 수 있습니다.".into());
        }
        let allowed: HashMap<PathBuf, &str> = project
            .document
            .assets
            .iter()
            .flat_map(|a| {
                [
                    (&a.asset.original_path, &a.asset.original_hash),
                    (&a.asset.file_path, &a.asset.current_hash),
                ]
            })
            .map(|(path, hash)| Ok((confined(&project.root, path)?, hash.as_str())))
            .collect::<Result<_>>()?;
        let mut seen = HashSet::new();
        paths
            .iter()
            .map(|path| {
                let canonical =
                    fs::canonicalize(path).map_err(|e| err("이미지를 첨부할 수 없습니다", e))?;
                let expected = allowed.get(&canonical).ok_or("현재 작업 폴더에 등록된 에셋만 Codex에 첨부할 수 있습니다.")?;
                if hash_file(&canonical)? != *expected {
                    return Err("검토할 이미지가 앱 밖에서 변경되었습니다. 프로젝트를 다시 불러온 뒤 검토하세요.".into());
                }
                if !seen.insert(canonical.clone()) {
                    return Err("같은 이미지가 중복 첨부되었습니다.".into());
                }
                Ok(canonical.to_string_lossy().into_owned())
            })
            .collect()
    }
}

#[tauri::command]
pub fn list_recent_projects() -> Result<Vec<RecentProject>> {
    recent_projects()
}
#[tauri::command]
pub fn workspace_changed(state: tauri::State<'_, EngineState>) -> Result<bool> {
    state.workspace_changed()
}
#[tauri::command]
pub fn workspace_reload(state: tauri::State<'_, EngineState>) -> Result<Workspace> {
    state.reload_workspace()
}
fn activate(app: &tauri::AppHandle, state: &EngineState, project: Project) -> Result<Workspace> {
    app.asset_protocol_scope()
        .allow_directory(&project.root, true)
        .map_err(|e| err("작업 폴더 미리보기를 허용할 수 없습니다", e))?;
    state.activate_project(project)
}
#[tauri::command]
pub fn workspace_current(
    app: tauri::AppHandle,
    state: tauri::State<'_, EngineState>,
) -> Result<Option<Workspace>> {
    if let Some(workspace) = state.current_workspace()? {
        return Ok(Some(workspace));
    }
    let Some(recent) = recent_projects()?.into_iter().next() else {
        return Ok(None);
    };
    activate(&app, &state, Project::open(Path::new(&recent.root))?).map(Some)
}
#[tauri::command]
pub fn workspace_create(
    app: tauri::AppHandle,
    state: tauri::State<'_, EngineState>,
    path: String,
    name: String,
) -> Result<Workspace> {
    activate(&app, &state, Project::create(Path::new(&path), &name)?)
}
#[tauri::command]
pub fn workspace_open(
    app: tauri::AppHandle,
    state: tauri::State<'_, EngineState>,
    path: String,
) -> Result<Workspace> {
    activate(&app, &state, Project::open(Path::new(&path))?)
}
#[tauri::command]
pub fn assets_import(
    state: tauri::State<'_, EngineState>,
    clipboard: tauri::State<'_, crate::clipboard::ClipboardState>,
    paths: Vec<String>,
    profile_id: String,
) -> Result<Workspace> {
    let workspace = state.import_assets(&paths, &profile_id)?;
    if let Err(error) = clipboard.release(&paths) {
        eprintln!("Imported images, but clipboard staging cleanup failed: {error}");
    }
    Ok(workspace)
}
#[tauri::command]
pub fn profile_save(
    state: tauri::State<'_, EngineState>,
    profile: RuleProfile,
) -> Result<Workspace> {
    state.save_profile(profile)
}
#[tauri::command]
pub fn project_notes_save(
    state: tauri::State<'_, EngineState>,
    notes: String,
) -> Result<Workspace> {
    state.save_notes(notes)
}
#[tauri::command]
pub fn asset_update(
    state: tauri::State<'_, EngineState>,
    asset_id: String,
    patch: AssetPatch,
) -> Result<Workspace> {
    state.update_asset(&asset_id, patch)
}
#[tauri::command]
pub fn assets_process(
    state: tauri::State<'_, EngineState>,
    asset_ids: Vec<String>,
    settings: ProcessSettings,
) -> Result<Workspace> {
    state.process_assets(&asset_ids, &settings)
}
#[tauri::command]
pub fn asset_reset(state: tauri::State<'_, EngineState>, asset_id: String) -> Result<Workspace> {
    state.reset_assets(&[asset_id])
}
#[tauri::command]
pub fn assets_export(
    state: tauri::State<'_, EngineState>,
    path: String,
    asset_ids: Vec<String>,
) -> Result<ExportResult> {
    state.export_assets(Path::new(&path), &asset_ids)
}

#[tauri::command]
pub fn assets_approve(
    state: tauri::State<'_, EngineState>,
    asset_ids: Vec<String>,
    design: bool,
    qc: bool,
) -> Result<Workspace> {
    state.approve_assets(&asset_ids, design, qc)
}

#[tauri::command]
pub fn asset_palette(
    state: tauri::State<'_, EngineState>,
    asset_id: String,
) -> Result<PaletteReport> {
    state.asset_palette(&asset_id)
}
#[tauri::command]
pub fn assets_palette_map(
    state: tauri::State<'_, EngineState>,
    asset_ids: Vec<String>,
) -> Result<Workspace> {
    state.map_palette_assets(&asset_ids)
}
#[tauri::command]
pub fn palette_read(path: String) -> Result<Vec<String>> {
    read_palette_file(Path::new(&path))
}
#[tauri::command]
pub fn assets_tile(
    state: tauri::State<'_, EngineState>,
    asset_ids: Vec<String>,
    settings: TileSettings,
) -> Result<Workspace> {
    state.tile_assets(&asset_ids, &settings)
}

#[tauri::command]
pub fn automation_plan(
    state: tauri::State<'_, EngineState>,
    asset_ids: Vec<String>,
) -> Result<AutomationPlan> {
    state.automation_plan(&asset_ids)
}

#[tauri::command]
pub fn assets_auto_prepare(
    state: tauri::State<'_, EngineState>,
    asset_ids: Vec<String>,
) -> Result<Workspace> {
    state.auto_prepare(&asset_ids)
}
