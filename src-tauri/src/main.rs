#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use resvera_core::JobOrchestrator;
use resvera_desktop::{commands::*, ipc_types::AppSettings, worker::QueueWorker, AppState};
use resvera_engine_ort::OrtEngine;
use resvera_persistence::AppDatabase;
use std::sync::{Arc, Mutex};
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().unwrap_or_else(|_| {
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_else(|_| ".".to_string());
                std::path::PathBuf::from(home).join(".resvera")
            });
            let app_cache_dir = app
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| app_data_dir.join("cache"));

            let _ = std::fs::create_dir_all(&app_data_dir);
            let _ = std::fs::create_dir_all(&app_cache_dir);

            let db_path = app_data_dir.join("resvera.db");
            let preview_dir = app_cache_dir.join("previews");
            let _ = std::fs::create_dir_all(&preview_dir);

            let models_dir = std::env::var_os("RESVERA_MODELS_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| app_data_dir.join("models"));
            let _ = std::fs::create_dir_all(&models_dir);

            let db = AppDatabase::open(&db_path).expect("Failed to initialize job database");
            let _ = db.run_crash_recovery_sweep();

            let engine = Arc::new(OrtEngine::new());
            let orchestrator =
                JobOrchestrator::with_models_root(db, engine, preview_dir, &models_dir);

            let settings_path = app_data_dir.join("settings.json");
            let initial_settings = if settings_path.exists() {
                std::fs::read_to_string(&settings_path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default()
            } else {
                AppSettings::default()
            };
            let settings = Arc::new(Mutex::new(initial_settings));

            let app_state = AppState {
                orchestrator,
                settings,
                settings_path,
            };

            // Start backend-owned queue worker and keep worker alive for app lifetime
            let worker = QueueWorker::start(app_state.clone());
            app.manage(app_state);
            app.manage(worker);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_status,
            list_models,
            create_upscale_job,
            create_batch_jobs,
            process_next_job,
            cancel_job,
            pause_queue,
            resume_queue,
            get_queue,
            get_job,
            get_jobs_history,
            load_settings,
            save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Resvera desktop application");
}
