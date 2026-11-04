use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
use std::{
    collections::HashSet,
    fs::{self, File, Metadata, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;

const MAX_PIXELS: u64 = 16_000_000;
const MAX_DIMENSION: u32 = 16_384;

#[derive(Default)]
pub struct ClipboardState {
    staged: Mutex<StagedImages>,
}

#[derive(Default)]
struct StagedImages {
    directory: Option<PathBuf>,
    issued: HashSet<PathBuf>,
}

fn validate_rgba(width: u32, height: u32, length: usize) -> Result<(), String> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 {
        return Err("클립보드 이미지의 가로·세로 크기가 비어 있습니다".into());
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION || pixels > MAX_PIXELS {
        return Err("클립보드 이미지는 1600만 화소 이하, 한 변 16384px 이하여야 합니다".into());
    }
    if pixels * 4 != length as u64 {
        return Err("클립보드 이미지의 RGBA 데이터 크기가 올바르지 않습니다".into());
    }
    Ok(())
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn ensure_directory(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)
            .map_err(|e| format!("클립보드 임시 폴더 확인 실패: {e}"))?;
        if is_link(&metadata) || !metadata.is_dir() {
            return Err("클립보드 임시 폴더에는 디렉터리 링크를 사용할 수 없습니다".into());
        }
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), String> {
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(format!("클립보드 임시 폴더 생성 실패: {error}")),
    }
    ensure_directory(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("클립보드 임시 폴더는 현재 사용자만 접근할 수 있어야 합니다".into());
        }
    }
    Ok(())
}

fn create_private_file(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|e| format!("클립보드 임시 이미지 생성 실패: {e}"))
}

impl StagedImages {
    fn directory(&mut self, base: &Path) -> Result<&Path, String> {
        if self.directory.is_none() {
            let metadata =
                fs::symlink_metadata(base).map_err(|e| format!("앱 데이터 폴더 확인 실패: {e}"))?;
            if is_link(&metadata) || !metadata.is_dir() {
                return Err("앱 데이터 폴더는 디렉터리 링크가 아닌 폴더여야 합니다".into());
            }
            let base = fs::canonicalize(base).map_err(|e| e.to_string())?;
            ensure_directory(&base)?;
            let cache = base.join("clipboard-paste");
            create_private_directory(&cache)?;
            let directory = cache.join(Uuid::new_v4().simple().to_string());
            create_private_directory(&directory)?;
            self.directory = Some(directory);
        }
        let directory = self
            .directory
            .as_deref()
            .ok_or("클립보드 임시 폴더가 없습니다")?;
        ensure_directory(directory)?;
        Ok(directory)
    }

    fn stage(
        &mut self,
        base: &Path,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<String, String> {
        validate_rgba(width, height, rgba.len())?;
        let filename = format!(
            "Clipboard_{}_{}_paste.png",
            chrono::Local::now().format("%m%d_%H%M%S"),
            &Uuid::new_v4().simple().to_string()[..8]
        );
        let path = self.directory(base)?.join(filename);
        let absolute_path = path
            .to_str()
            .ok_or("클립보드 임시 경로가 올바른 UTF-8이 아닙니다")?
            .to_owned();
        let file = create_private_file(&path)?;
        self.issued.insert(path.clone());
        let result = (|| {
            let mut writer = BufWriter::new(file);
            PngEncoder::new(&mut writer)
                .write_image(rgba, width, height, ExtendedColorType::Rgba8)
                .map_err(|e| format!("클립보드 PNG 변환 실패: {e}"))?;
            writer
                .flush()
                .map_err(|e| format!("클립보드 PNG 저장 실패: {e}"))
        })();
        if let Err(error) = result {
            let _ = self.release_one(&path);
            return Err(error);
        }
        Ok(absolute_path)
    }

    fn remove_owned_file(&self, path: &Path) -> Result<(), String> {
        let directory = self
            .directory
            .as_deref()
            .ok_or("클립보드 임시 폴더가 없습니다")?;
        ensure_directory(directory)?;
        if path.parent() != Some(directory) {
            return Err("앱이 생성한 클립보드 이미지가 아닙니다".into());
        }
        // remove_file unlinks an owned file (or a replacement symlink), never its target.
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("클립보드 임시 이미지 정리 실패: {error}")),
        }
        Ok(())
    }

    fn release_one(&mut self, path: &Path) -> Result<(), String> {
        let Some(owned) = self.issued.get(path) else {
            return Ok(());
        };
        self.remove_owned_file(owned)?;
        self.issued.remove(path);
        Ok(())
    }

    fn remove_empty_directory(&mut self) {
        if !self.issued.is_empty() {
            return;
        }
        if let Some(directory) = &self.directory {
            if ensure_directory(directory).is_ok() && fs::remove_dir(directory).is_ok() {
                self.directory = None;
            }
        }
    }

    fn release(&mut self, paths: &[String]) -> Result<(), String> {
        let mut failure = None;
        for path in paths {
            if let Err(error) = self.release_one(Path::new(path)) {
                failure.get_or_insert(error);
            }
        }
        self.remove_empty_directory();
        failure.map_or(Ok(()), Err)
    }

    fn release_all(&mut self) -> Result<(), String> {
        let mut failure = None;
        for path in std::mem::take(&mut self.issued) {
            if let Err(error) = self.remove_owned_file(&path) {
                self.issued.insert(path);
                failure.get_or_insert(error);
            }
        }
        self.remove_empty_directory();
        failure.map_or(Ok(()), Err)
    }
}

impl ClipboardState {
    /// Only paths issued by this state can be removed; ordinary import paths are ignored.
    pub fn release(&self, paths: &[String]) -> Result<(), String> {
        self.staged
            .lock()
            .map_err(|_| "클립보드 임시 이미지 상태를 읽을 수 없습니다")?
            .release(paths)
    }

    /// Also call this on Tauri's Exit event, where managed state may not be dropped.
    pub fn release_all(&self) -> Result<(), String> {
        self.staged
            .lock()
            .map_err(|_| "클립보드 임시 이미지 상태를 읽을 수 없습니다")?
            .release_all()
    }
}

impl Drop for ClipboardState {
    fn drop(&mut self) {
        let staged = self
            .staged
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        let _ = staged.release_all();
    }
}

/// Invoked only for an explicit paste shortcut or the paste button, never on focus/startup.
#[tauri::command]
pub async fn capture_clipboard_image(
    app: AppHandle,
    _state: State<'_, ClipboardState>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let image = app.clipboard().read_image().map_err(|error| {
            format!("클립보드에 이미지가 없거나 이미지를 읽을 수 없습니다. 이미지를 복사한 뒤 다시 붙여넣으세요. ({error})")
        })?;
        let base = crate::settings::data_dir()?;
        let state = app.state::<ClipboardState>();
        let result = state.staged.lock()
            .map_err(|_| "클립보드 임시 이미지 상태를 읽을 수 없습니다")?
            .stage(&base, image.rgba(), image.width(), image.height());
        result
    })
    .await
    .map_err(|error| format!("클립보드 이미지 처리 실패: {error}"))?
}

#[tauri::command]
pub fn release_clipboard_images(
    state: State<'_, ClipboardState>,
    paths: Vec<String>,
) -> Result<(), String> {
    state.release(&paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!("taid-clipboard-test-{}", Uuid::new_v4()));
            create_private_directory(&path).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rgba_boundaries_are_checked_without_allocating_pixels() {
        assert!(validate_rgba(4000, 4000, 64_000_000).is_ok());
        assert!(validate_rgba(16_384, 1, 65_536).is_ok());
        assert!(validate_rgba(0, 1, 0).is_err());
        assert!(validate_rgba(4001, 4000, 64_016_000).is_err());
        assert!(validate_rgba(16_385, 1, 65_540).is_err());
        assert!(validate_rgba(1, 1, 3).is_err());
        assert!(validate_rgba(u32::MAX, u32::MAX, 0).is_err());
    }

    #[test]
    fn staged_png_preserves_rgba_and_release_ignores_unissued_paths() {
        let base = TestDirectory::new();
        let unrelated = base.0.join("unrelated.png");
        fs::write(&unrelated, b"keep").unwrap();
        let mut staged = StagedImages::default();
        let rgba = [12, 34, 56, 78, 90, 123, 145, 255];
        let path = staged.stage(&base.0, &rgba, 2, 1).unwrap();
        assert!(Path::new(&path).is_absolute());
        assert!(path.ends_with("_paste.png"));
        let decoded = image::open(&path).unwrap().into_rgba8();
        assert_eq!(decoded.as_raw(), &rgba);
        staged
            .release(&[unrelated.to_str().unwrap().into()])
            .unwrap();
        assert!(unrelated.exists());
        staged.release(&[path.clone(), path.clone()]).unwrap();
        assert!(!Path::new(&path).exists());
        assert!(staged.directory.is_none());
    }

    #[test]
    fn state_drop_removes_only_its_owned_files() {
        let base = TestDirectory::new();
        let state = ClipboardState::default();
        let path = state
            .staged
            .lock()
            .unwrap()
            .stage(&base.0, &[0, 0, 0, 255], 1, 1)
            .unwrap();
        drop(state);
        assert!(!Path::new(&path).exists());
        assert!(base.0.exists());
    }

    #[cfg(unix)]
    #[test]
    fn directory_links_cannot_redirect_capture_or_cleanup() {
        use std::os::unix::fs::symlink;
        let base = TestDirectory::new();
        let outside = TestDirectory::new();
        symlink(&outside.0, base.0.join("clipboard-paste")).unwrap();
        let mut staged = StagedImages::default();
        assert!(staged.stage(&base.0, &[0, 0, 0, 255], 1, 1).is_err());
        fs::remove_file(base.0.join("clipboard-paste")).unwrap();
        let path = staged.stage(&base.0, &[0, 0, 0, 255], 1, 1).unwrap();
        let directory = staged.directory.clone().unwrap();
        let moved = directory.with_extension("moved");
        fs::rename(&directory, &moved).unwrap();
        let target = outside.0.join(Path::new(&path).file_name().unwrap());
        fs::write(&target, b"keep").unwrap();
        symlink(&outside.0, &directory).unwrap();
        assert!(staged.release(&[path]).is_err());
        assert_eq!(fs::read(target).unwrap(), b"keep");
    }
}
