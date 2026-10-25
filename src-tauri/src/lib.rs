//! AIMeetings: record calls and meetings, transcribe them on this computer with who said what, and
//! analyze them with a CLI AI provider or a local model.

pub mod applog;
pub mod asr;
pub mod commands;
pub mod db;
pub mod dictation;
pub mod download;
pub mod hardware;
pub mod llm;
pub mod meetings;
pub mod paths;
pub mod power;
pub mod proc;
pub mod providers;
pub mod secrets;
pub mod shell_env;
pub mod tray;
pub mod vad_onnx;
pub mod voice_fit;

use tauri::Manager;

use commands::{app_cmd, dictation_cmd, media_cmd, meetings_cmd, models_cmd, providers_cmd};

/// Lets the libraries downloaded at run time (whisper.cpp) find the Visual C++ runtime the
/// installer ships beside the app (`resources/vcrt`), on a Windows that never installed the
/// redistributable — common on Windows on ARM. They are loaded with
/// `LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`, which includes directories added here.
#[cfg(windows)]
fn add_dll_directory(dir: &std::path::Path) {
    use std::os::windows::ffi::OsStrExt;
    if !dir.is_dir() {
        return;
    }
    let wide: Vec<u16> = dunce::simplified(dir).as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated wide path that outlives the call.
    unsafe {
        windows_sys::Win32::System::LibraryLoader::AddDllDirectory(wide.as_ptr());
    }
}

pub fn run() {
    // First, before any thread exists: a GUI app on macOS starts with launchd's PATH, which knows
    // none of the places the AI CLIs install into.
    shell_env::import_login_path();
    let _ = paths::ensure_dirs();
    applog::init();
    applog::install_panic_hook();

    let db = match db::open() {
        Ok(db) => db,
        Err(error) => {
            applog::error(&format!("database: {error}"));
            panic!("AIMeetings couldn't open its database: {error}");
        }
    };
    {
        let conn = db.0.lock().expect("fresh connection");
        let level = db::queries::get_setting(&conn, "power_saving").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0);
        power::set_saving(level);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // A meeting's recording, streamed from disk to the player — see `meetings_cmd::serve_audio`.
        .register_asynchronous_uri_scheme_protocol("aim-audio", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || responder.respond(meetings_cmd::serve_audio(&app, &request)));
        })
        .manage(db)
        .manage(meetings_cmd::MeetingsState::default())
        .manage(dictation_cmd::DictationRegistry::default())
        .manage(models_cmd::LlmRegistry::default())
        .setup(|app| {
            if let Ok(dir) = app.path().resource_dir() {
                #[cfg(windows)]
                add_dll_directory(&dir.join("vcrt"));
                paths::set_resource_dir(dir);
            }
            llm::server::attach(app.handle().clone());
            // Off the startup path: listing processes takes a moment.
            std::thread::spawn(|| {
                providers::sweep_stale_prompts();
                llm::server::sweep_stale();
                llm::server::sweep_orphans();
                llm::models::sweep_partials();
                // Measured once per session (memory speed, the GPU): done now, so the Models screen
                // opens with its figures ready instead of waiting for them.
                let _ = hardware::machine::machine_blocking();
            });
            meetings_cmd::start(app.handle());
            tray::install(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the app in the tray while a meeting records or is processed.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && meetings_cmd::busy_count() > 0 {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_cmd::get_settings,
            app_cmd::set_setting,
            app_cmd::write_file_bytes,
            app_cmd::reveal_path,
            app_cmd::open_external,
            app_cmd::app_diagnostics,
            app_cmd::prepare_update,
            media_cmd::media_extract_audio,
            meetings_cmd::meetings_status,
            meetings_cmd::meetings_install,
            meetings_cmd::meetings_cancel_install,
            meetings_cmd::meetings_remove_voices,
            meetings_cmd::meetings_set_cloud_key,
            meetings_cmd::meetings_test_cloud,
            meetings_cmd::meetings_bench,
            meetings_cmd::meetings_system_audio_settings,
            meetings_cmd::meetings_reveal_recordings,
            meetings_cmd::meetings_start,
            meetings_cmd::meetings_monitor,
            meetings_cmd::meetings_set_mic_muted,
            meetings_cmd::meetings_set_mic_gain,
            meetings_cmd::meetings_pause,
            meetings_cmd::meetings_stop,
            meetings_cmd::meetings_discard,
            meetings_cmd::meetings_delete,
            meetings_cmd::meetings_cancel_job,
            meetings_cmd::meetings_reprocess,
            meetings_cmd::meetings_transcribe,
            meetings_cmd::meetings_import,
            meetings_cmd::meetings_list,
            meetings_cmd::meetings_search_lines,
            meetings_cmd::meetings_set_title,
            meetings_cmd::meetings_set_notes,
            meetings_cmd::meetings_append_notes,
            meetings_cmd::meetings_move,
            meetings_cmd::meetings_set_local_only,
            meetings_cmd::folders_list,
            meetings_cmd::folders_create,
            meetings_cmd::folders_rename,
            meetings_cmd::folders_delete,
            meetings_cmd::folders_set_local_only,
            meetings_cmd::meetings_detail,
            meetings_cmd::meetings_audio,
            meetings_cmd::meetings_peaks,
            meetings_cmd::meetings_export_audio,
            meetings_cmd::meetings_delete_audio,
            meetings_cmd::meetings_update_line,
            meetings_cmd::meetings_set_line_speaker,
            meetings_cmd::meetings_rename_speaker,
            meetings_cmd::meetings_merge_speakers,
            meetings_cmd::meetings_add_speaker,
            meetings_cmd::meetings_voices,
            meetings_cmd::meetings_save_voice,
            meetings_cmd::meetings_rename_voice,
            meetings_cmd::meetings_delete_voice,
            meetings_cmd::meetings_recipes,
            meetings_cmd::meetings_save_recipe,
            meetings_cmd::meetings_delete_recipe,
            meetings_cmd::meetings_ai,
            dictation_cmd::dictation_status,
            dictation_cmd::dictation_install,
            dictation_cmd::dictation_cancel_install,
            dictation_cmd::dictation_remove,
            dictation_cmd::dictation_inputs,
            dictation_cmd::dictation_mic_permission,
            dictation_cmd::dictation_mic_request,
            dictation_cmd::dictation_mic_settings,
            models_cmd::models_overview,
            models_cmd::speech_measure,
            models_cmd::llm_install,
            models_cmd::llm_cancel_install,
            models_cmd::llm_delete,
            models_cmd::llm_set_config,
            models_cmd::llm_server_models,
            models_cmd::llm_probe,
            providers_cmd::providers_list,
            providers_cmd::providers_set_builtin,
            providers_cmd::providers_save_custom,
            providers_cmd::providers_delete_custom,
            providers_cmd::providers_select,
            providers_cmd::providers_test,
            providers_cmd::providers_cancel_run,
        ])
        .build(tauri::generate_context!())
        .expect("error while building AIMeetings")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                meetings_cmd::shutdown(app);
                tauri::async_runtime::block_on(llm::server::shutdown());
                applog::mark_clean_exit();
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::show_main_window(app);
            }
        });
}
