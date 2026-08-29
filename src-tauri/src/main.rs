#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use resvera_core::JobOrchestrator;
use resvera_desktop::{commands::*, ipc_types::AppSettings, AppState};
use resvera_engine_ort::OrtEngine;
use resvera_persistence::AppDatabase;
use std::sync::{Arc, Mutex};

fn main() {
    let engine = Arc::new(OrtEngine::new());
    let temp_db_path = std::env::temp_dir().join("resvera_app.db");
    let preview_dir = std::env::temp_dir().join("resvera_previews");
    let db = AppDatabase::open(temp_db_path).expect("Failed to initialize job database");
    let orchestrator = JobOrchestrator::new(db, engine, preview_dir);
    let settings = Arc::new(Mutex::new(AppSettings::default()));

    let app_state = AppState {
        orchestrator,
        settings,
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_runtime_status,
            list_models,
            create_upscale_job,
            create_batch_jobs,
            cancel_job,
            pause_queue,
            resume_queue,
            get_queue,
            get_job,
            load_settings,
            save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Resvera desktop application");
}
