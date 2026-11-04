use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const APP_ID: &str = "com.a7garden.taid-designer";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSettings {
    pub model: String,
    pub effort: String,
}
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            model: String::new(),
            effort: "low".into(),
        }
    }
}
pub fn data_dir() -> Result<PathBuf, String> {
    let path = if let Some(path) = std::env::var_os("TAID_DATA_DIR") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("TAID_DATA_DIR에는 절대 경로를 지정하세요".into());
        }
        path
    } else {
        dirs::data_dir()
            .ok_or("앱 데이터 폴더를 찾을 수 없습니다")?
            .join(APP_ID)
    };
    fs::create_dir_all(&path).map_err(|e| format!("앱 데이터 폴더 생성 실패: {e}"))?;
    Ok(path)
}
fn validate(settings: &AppSettings) -> Result<(), String> {
    if settings.model.len() > 160
        || !settings
            .model
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'/' | b':'))
    {
        return Err("모델 ID는 영문·숫자·하이픈·밑줄·점·슬래시·콜론으로 입력하세요".into());
    }
    if !["none", "minimal", "low", "medium", "high", "xhigh"].contains(&settings.effort.as_str()) {
        return Err("지원하지 않는 추론 강도입니다".into());
    }
    Ok(())
}
fn load_from(directory: &Path) -> Result<AppSettings, String> {
    let path = directory.join("settings.json");
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 32_768 {
        return Err("설정 파일이 일반 파일이 아니거나 너무 큽니다".into());
    }
    let bytes = fs::read(&path).map_err(|e| format!("설정 읽기 실패: {e}"))?;
    let settings =
        serde_json::from_slice(&bytes).map_err(|e| format!("설정 파일 해석 실패: {e}"))?;
    validate(&settings)?;
    Ok(settings)
}
pub fn load() -> Result<AppSettings, String> {
    load_from(&data_dir()?)
}
fn write_to(directory: &Path, settings: &AppSettings) -> Result<(), String> {
    validate(settings)?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    let temp = directory.join(format!(".settings-{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temp, directory.join("settings.json"))
            .map_err(|e| format!("설정 저장 실패: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
fn with_lock<T>(directory: &Path, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(".settings.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "다른 프로세스가 설정을 저장하고 있습니다. 완료 후 다시 시도하세요")?;
    action()
}
pub fn save(settings: AppSettings) -> Result<AppSettings, String> {
    let directory = data_dir()?;
    with_lock(&directory, || {
        write_to(&directory, &settings)?;
        Ok(settings)
    })
}
pub fn update(model: Option<String>, effort: Option<String>) -> Result<AppSettings, String> {
    let directory = data_dir()?;
    with_lock(&directory, || {
        let mut settings = load_from(&directory)?;
        if let Some(model) = model {
            settings.model = model;
        }
        if let Some(effort) = effort {
            settings.effort = effort;
        }
        write_to(&directory, &settings)?;
        Ok(settings)
    })
}
#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    load()
}
#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<AppSettings, String> {
    save(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn temp() -> Temp {
        let p = std::env::temp_dir().join(format!("taid-settings-{}", Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        Temp(p)
    }
    #[test]
    fn invalid_setting_does_not_replace_saved_selection() {
        let dir = temp();
        let original = AppSettings {
            model: "gpt-6.1-sol".into(),
            effort: "medium".into(),
        };
        write_to(&dir.0, &original).unwrap();
        let invalid = AppSettings {
            model: "gpt;run-command".into(),
            effort: "medium".into(),
        };
        assert!(write_to(&dir.0, &invalid).is_err());
        assert_eq!(load_from(&dir.0).unwrap(), original);
    }
    #[test]
    fn concurrent_settings_writer_is_refused() {
        let dir = temp();
        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.0.join(".settings.lock"))
            .unwrap();
        held.try_lock_exclusive().unwrap();
        assert!(with_lock(&dir.0, || Ok(())).is_err());
        drop(held);
        assert!(with_lock(&dir.0, || Ok(())).is_ok());
    }
}
