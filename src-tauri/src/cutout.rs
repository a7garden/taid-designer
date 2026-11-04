//! Local U²-Net-p salient-object segmentation, shared by desktop and CLI.
//! Model weights: Xuebin Qin et al., Apache-2.0; rembg is only the ONNX host.
//! Tensor contract follows rembg/sessions/{base,u2netp}.py: 320² RGB NCHW,
//! image-maximum scaling, ImageNet normalization, first fused output + min/max.
use crate::{
    engine::{EngineState, Workspace},
    settings,
};
use fs2::FileExt;
use image::{
    imageops::{self, FilterType},
    GrayImage, ImageFormat, RgbImage, RgbaImage,
};
use ort::{
    session::{run_options::RunOptions, Session},
    value::Tensor,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use uuid::Uuid;

type Result<T, E = String> = std::result::Result<T, E>;
const MODEL: &str = "U2Netp";
const MODEL_BYTES: u64 = 4_574_861;
const MODEL_SHA256: &str = "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8";
const MODEL_REVISION: &str =
    "rembg-v0.0.0/sha256:309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8";
const MODEL_URL: &str = "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx";
const LICENSE_URL: &str = "https://github.com/xuebinqin/U-2-Net/blob/master/LICENSE";
const SIDE: u32 = 320;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CutoutProgress {
    pub stage: String,
    pub completed: usize,
    pub total: usize,
    pub downloaded: u64,
    pub total_bytes: Option<u64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CutoutStatus {
    pub ready: bool,
    pub busy: bool,
    pub model: &'static str,
    pub license: &'static str,
    pub license_url: &'static str,
    pub download_bytes: u64,
    pub revision: &'static str,
    pub notice: &'static str,
}
#[derive(Default)]
pub struct CutoutState {
    // One atomic state avoids losing a cancellation between reserving a job and
    // resetting a separate cancellation flag: 0 idle, 1 running, 2 cancelled.
    phase: AtomicU8,
}
struct BusyGuard<'a>(&'a AtomicU8);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(0, Ordering::Release);
    }
}
struct TemporaryPath(PathBuf, bool);
impl Drop for TemporaryPath {
    fn drop(&mut self) {
        if self.1 {
            let _ = fs::remove_dir_all(&self.0);
        } else {
            let _ = fs::remove_file(&self.0);
        }
    }
}

fn model_path() -> Result<PathBuf> {
    Ok(settings::data_dir()?
        .join("models")
        .join("u2netp")
        .join(MODEL_SHA256)
        .join("u2netp.onnx"))
}
fn model_valid(path: &Path) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("모델 캐시를 읽을 수 없습니다: {error}")),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != MODEL_BYTES {
        return Ok(false);
    }
    let mut file = File::open(path).map_err(|error| format!("모델을 읽을 수 없습니다: {error}"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("모델 해시를 읽을 수 없습니다: {error}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()) == MODEL_SHA256)
}
#[tauri::command]
pub fn cutout_status(state: tauri::State<'_, CutoutState>) -> Result<CutoutStatus> {
    Ok(CutoutStatus {
        ready: model_valid(&model_path()?)?,
        busy: state.phase.load(Ordering::Acquire) != 0,
        model: MODEL,
        license: "Apache-2.0",
        license_url: LICENSE_URL,
        download_bytes: MODEL_BYTES,
        notice: include_str!("cutout/NOTICE.txt"),
        revision: MODEL_REVISION,
    })
}
fn check_cancelled(control: &CutoutState) -> Result<()> {
    if control.phase.load(Ordering::Acquire) == 2 {
        Err("자동 누끼를 취소했습니다. 원본과 프로젝트는 변경하지 않았습니다.".into())
    } else {
        Ok(())
    }
}
fn progress(stage: &str, completed: usize, total: usize, downloaded: u64) -> CutoutProgress {
    CutoutProgress {
        stage: stage.into(),
        completed,
        total,
        downloaded,
        total_bytes: Some(MODEL_BYTES),
    }
}
fn ensure_model(
    control: &CutoutState,
    total: usize,
    emit: &mut impl FnMut(CutoutProgress),
) -> Result<(PathBuf, u64)> {
    let path = model_path()?;
    if model_valid(&path)? {
        return Ok((path, 0));
    }
    let parent = path.parent().ok_or("모델 캐시 경로가 올바르지 않습니다.")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("모델 캐시 폴더를 만들 수 없습니다: {error}"))?;
    let lock_path = parent.join(".download.lock");
    if fs::symlink_metadata(&lock_path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("모델 다운로드 잠금은 심볼릭 링크일 수 없습니다.".into());
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|error| format!("모델 캐시를 잠글 수 없습니다: {error}"))?;
    lock.try_lock_exclusive()
        .map_err(|_| "다른 Taid 창이나 CLI가 모델을 다운로드 중입니다. 완료 후 다시 실행하세요.")?;
    if model_valid(&path)? {
        return Ok((path, 0));
    }
    check_cancelled(control)?;
    emit(progress("download", 0, total, 0));
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .user_agent("Taid-Designer/1.0 local-model-download")
        .build()
        .map_err(|error| format!("모델 다운로드를 시작할 수 없습니다: {error}"))?;
    // Only this fixed public model URL is requested. No image, path, project,
    // token, prompt, or user-controlled payload enters any network request.
    let mut response = client
        .get(MODEL_URL)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("U2Netp 모델 다운로드 실패 (이미지는 전송하지 않음): {error}"))?;
    if response
        .content_length()
        .is_some_and(|size| size != MODEL_BYTES)
    {
        return Err(
            "다운로드 서버의 모델 크기가 고정된 버전과 다릅니다. 저장하지 않았습니다.".into(),
        );
    }
    let temporary = TemporaryPath(parent.join(format!(".{}.partial", Uuid::new_v4())), false);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary.0)
        .map_err(|error| format!("모델 임시 파일을 만들 수 없습니다: {error}"))?;
    let mut hash = Sha256::new();
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 65_536];
    loop {
        check_cancelled(control)?;
        let count = response
            .read(&mut buffer)
            .map_err(|error| format!("모델 다운로드가 중단되었습니다: {error}"))?;
        if count == 0 {
            break;
        }
        downloaded += count as u64;
        if downloaded > MODEL_BYTES {
            return Err("모델 다운로드가 예상 크기를 초과했습니다.".into());
        }
        hash.update(&buffer[..count]);
        file.write_all(&buffer[..count])
            .map_err(|error| format!("모델을 저장할 수 없습니다: {error}"))?;
        emit(progress("download", 0, total, downloaded));
    }
    if downloaded != MODEL_BYTES || format!("{:x}", hash.finalize()) != MODEL_SHA256 {
        return Err(
            "모델 SHA-256 검증에 실패했습니다. 다운로드를 폐기했으며 누끼를 실행하지 않았습니다."
                .into(),
        );
    }
    check_cancelled(control)?;
    file.sync_all()
        .map_err(|error| format!("모델 저장을 완료할 수 없습니다: {error}"))?;
    drop(file);
    if path.exists() {
        if !fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        {
            return Err("모델 캐시 경로가 일반 파일이 아닙니다.".into());
        }
        fs::remove_file(&path)
            .map_err(|error| format!("손상된 모델 캐시를 교체할 수 없습니다: {error}"))?;
    }
    fs::rename(&temporary.0, &path).map_err(|error| format!("모델 캐시 저장 실패: {error}"))?;
    Ok((path, downloaded))
}

fn normalized_input(image: &RgbaImage) -> Vec<f32> {
    // Match RGB conversion (ignore alpha for the network only), not premultiplied
    // RGBA resizing. The output always retains the untouched full-resolution RGB.
    let rgb = RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let pixel = image.get_pixel(x, y);
        image::Rgb([pixel[0], pixel[1], pixel[2]])
    });
    let resized = imageops::resize(&rgb, SIDE, SIDE, FilterType::Lanczos3);
    let maximum = resized.as_raw().iter().copied().max().unwrap_or(0).max(1) as f32;
    let plane = (SIDE * SIDE) as usize;
    let mut input = vec![0.0f32; plane * 3];
    let mean = [0.485, 0.456, 0.406];
    let standard_deviation = [0.229, 0.224, 0.225];
    for (index, pixel) in resized.pixels().enumerate() {
        for channel in 0..3 {
            input[channel * plane + index] =
                (f32::from(pixel[channel]) / maximum - mean[channel]) / standard_deviation[channel];
        }
    }
    input
}

fn apply_mask(mut source: RgbaImage, prediction: &[f32]) -> Result<RgbaImage> {
    if prediction.len() != (SIDE * SIDE) as usize
        || prediction.iter().any(|value| !value.is_finite())
    {
        return Err("모델이 유효한 320×320 마스크를 반환하지 않았습니다.".into());
    }
    let minimum = prediction.iter().copied().fold(f32::INFINITY, f32::min);
    let maximum = prediction.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let range = maximum - minimum;
    if range <= f32::EPSILON {
        return Err(
            "모델이 일정한 마스크만 반환했습니다. 자동 누끼 결과를 저장하지 않았습니다.".into(),
        );
    }
    let mask = GrayImage::from_raw(
        SIDE,
        SIDE,
        prediction
            .iter()
            .map(|value| (((value - minimum) / range).clamp(0.0, 1.0) * 255.0) as u8)
            .collect(),
    )
    .ok_or("누끼 마스크 크기가 올바르지 않습니다.")?;
    let mask = imageops::resize(&mask, source.width(), source.height(), FilterType::Lanczos3);
    for (pixel, coverage) in source.pixels_mut().zip(mask.pixels()) {
        pixel[3] = ((u16::from(pixel[3]) * u16::from(coverage[0]) + 127) / 255) as u8;
    }
    Ok(source)
}

fn segment(session: &mut Session, source: RgbaImage, control: &CutoutState) -> Result<RgbaImage> {
    check_cancelled(control)?;
    let tensor = Tensor::from_array((
        [1usize, 3, SIDE as usize, SIDE as usize],
        normalized_input(&source),
    ))
    .map_err(|error| format!("누끼 입력을 준비할 수 없습니다: {error}"))?;
    let options =
        RunOptions::new().map_err(|error| format!("누끼 실행을 준비할 수 없습니다: {error}"))?;
    let done = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            let started = Instant::now();
            while !done.load(Ordering::Acquire) {
                if control.phase.load(Ordering::Acquire) == 2
                    || started.elapsed() > Duration::from_secs(120)
                {
                    let _ = options.terminate();
                    break;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
        });
        let result = session.run_with_options(ort::inputs![tensor], &options);
        done.store(true, Ordering::Release);
        result
    });
    check_cancelled(control)?;
    let outputs = result
        .map_err(|error| format!("로컬 누끼 추론 실패 (최대 120초, 대체 처리 없음): {error}"))?;
    let first = outputs
        .iter()
        .next()
        .ok_or("모델이 마스크를 반환하지 않았습니다.")?
        .1;
    let (shape, data) = first
        .try_extract_tensor::<f32>()
        .map_err(|error| format!("누끼 마스크 형식이 올바르지 않습니다: {error}"))?;
    if shape[..] != [1i64, 1, SIDE as i64, SIDE as i64] {
        return Err(format!("예상하지 못한 누끼 출력 크기: {shape:?}"));
    }
    apply_mask(source, data)
}

/// Explicit action only. Download/cache/model inference are identical for GUI and
/// CLI; no background initialization downloads, Python, paid API, or fallback.
pub fn remove(
    state: &EngineState,
    ids: &[String],
    control: &CutoutState,
    emit: impl FnMut(CutoutProgress),
) -> Result<Workspace> {
    begin(control)?;
    let _busy = BusyGuard(&control.phase);
    remove_started(state, ids, control, emit)
}

fn begin(control: &CutoutState) -> Result<()> {
    control
        .phase
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| ())
        .map_err(|_| "다른 자동 누끼 작업이 실행 중입니다.".into())
}

fn remove_started(
    state: &EngineState,
    ids: &[String],
    control: &CutoutState,
    mut emit: impl FnMut(CutoutProgress),
) -> Result<Workspace> {
    let snapshot = state.prepare_variants(ids)?;
    let total = snapshot.assets.len();
    emit(progress("prepare", 0, total, 0));
    let (model, downloaded) = ensure_model(control, total, &mut emit)?;
    check_cancelled(control)?;
    emit(progress("load-model", 0, total, downloaded));
    let threads = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .clamp(1, 4);
    let mut session = Session::builder()
        .and_then(|builder| builder.with_intra_threads(threads))
        .and_then(|builder| builder.commit_from_file(&model))
        .map_err(|error| format!("로컬 ONNX Runtime 또는 U2Netp 모델을 열 수 없습니다: {error}"))?;
    let staging_root = settings::data_dir()?.join("cutout-staging");
    fs::create_dir_all(&staging_root)
        .map_err(|error| format!("누끼 임시 폴더를 만들 수 없습니다: {error}"))?;
    let staging = TemporaryPath(staging_root.join(Uuid::new_v4().to_string()), true);
    fs::create_dir(&staging.0)
        .map_err(|error| format!("누끼 임시 폴더를 만들 수 없습니다: {error}"))?;
    let mut prepared = Vec::with_capacity(total);
    for index in 0..total {
        check_cancelled(control)?;
        emit(progress("segment", index, total, downloaded));
        let output = segment(&mut session, snapshot.load_image(index)?, control)?;
        let path = staging.0.join(format!("{index}.png"));
        output
            .save_with_format(&path, ImageFormat::Png)
            .map_err(|error| format!("누끼 결과를 임시 저장할 수 없습니다: {error}"))?;
        prepared.push(path);
        emit(progress("segment", index + 1, total, downloaded));
    }
    check_cancelled(control)?;
    emit(progress("commit", total, total, downloaded));
    let workspace = state.add_cutout_variants(&snapshot, &prepared, MODEL, MODEL_REVISION)?;
    emit(progress("done", total, total, downloaded));
    Ok(workspace)
}

#[tauri::command]
pub fn cancel_cutout(state: tauri::State<'_, CutoutState>) {
    let _ = state
        .phase
        .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire);
}
#[tauri::command]
pub async fn remove_background(app: tauri::AppHandle, asset_ids: Vec<String>) -> Result<Workspace> {
    begin(&app.state::<CutoutState>())?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<EngineState>();
        let control = app.state::<CutoutState>();
        let _busy = BusyGuard(&control.phase);
        remove_started(&state, &asset_ids, &control, |event| {
            let _ = app.emit("cutout-progress", event);
        })
    })
    .await
    .map_err(|error| format!("로컬 누끼 작업을 완료할 수 없습니다: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mask_preserves_canvas_rgb_and_multiplies_existing_alpha() {
        let source = RgbaImage::from_fn(SIDE, SIDE, |x, y| {
            image::Rgba([x as u8, y as u8, 37, if x == 0 { 0 } else { 128 }])
        });
        let prediction: Vec<_> = (0..SIDE * SIDE)
            .map(|index| (index % SIDE) as f32 / (SIDE - 1) as f32)
            .collect();
        let output = apply_mask(source.clone(), &prediction).unwrap();
        assert_eq!(output.dimensions(), source.dimensions());
        for (before, after) in source.pixels().zip(output.pixels()) {
            assert_eq!(before.0[..3], after.0[..3]);
            assert!(after[3] <= before[3]);
        }
        assert_eq!(output.get_pixel(0, 7)[3], 0);
        assert_eq!(output.get_pixel(SIDE - 1, 7)[3], 128);
        let coverage = (160.0f32 / 319.0 * 255.0) as u8;
        assert_eq!(
            output.get_pixel(160, 7)[3],
            ((128u16 * u16::from(coverage) + 127) / 255) as u8
        );
    }
}
