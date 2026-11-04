pub mod cli;
pub mod clipboard;
pub mod codex;
pub mod cutout;
pub mod engine;
pub mod settings;

#[tauri::command]
fn copy_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    if text.len() > 256_000 {
        return Err("복사할 텍스트가 너무 깁니다".into());
    }
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("클립보드 복사 실패: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(engine::EngineState::default())
        .manage(codex::CodexState::default())
        .manage(clipboard::ClipboardState::default())
        .manage(cutout::CutoutState::default())
        .invoke_handler(tauri::generate_handler![
            copy_text,
            clipboard::capture_clipboard_image,
            clipboard::release_clipboard_images,
            cutout::cutout_status,
            cutout::remove_background,
            cutout::cancel_cutout,
            engine::workspace_current,
            engine::workspace_create,
            engine::workspace_open,
            engine::list_recent_projects,
            engine::workspace_changed,
            engine::workspace_reload,
            engine::assets_import,
            engine::profile_save,
            engine::project_notes_save,
            engine::asset_update,
            engine::assets_process,
            engine::asset_reset,
            engine::assets_approve,
            engine::assets_export,
            engine::asset_palette,
            engine::assets_palette_map,
            engine::assets_tile,
            engine::palette_read,
            codex::codex_status,
            codex::codex_start,
            codex::codex_cancel,
            codex::codex_models,
            settings::get_settings,
            settings::save_settings,
        ])
        .build(tauri::generate_context!())
        .expect("Taid native host could not start")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                use tauri::Manager;
                if let Err(error) = app.state::<clipboard::ClipboardState>().release_all() {
                    eprintln!("클립보드 임시 이미지 정리 실패: {error}");
                }
            }
        });
}
