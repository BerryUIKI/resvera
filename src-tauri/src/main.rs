#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use resvera_core::JobOrchestrator;
use resvera_desktop::{commands::*, ipc_types::AppSettings, worker::QueueWorker, AppState};
use resvera_engine_ort::OrtEngine;
use resvera_persistence::AppDatabase;
use std::path::PathBuf;
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

            std::fs::create_dir_all(&app_data_dir).map_err(|e| {
                format!(
                    "Failed to create app data directory '{}': {}",
                    app_data_dir.display(),
                    e
                )
            })?;
            std::fs::create_dir_all(&app_cache_dir).map_err(|e| {
                format!(
                    "Failed to create app cache directory '{}': {}",
                    app_cache_dir.display(),
                    e
                )
            })?;

            let db_path = app_data_dir.join("resvera.db");
            let preview_dir = app_cache_dir.join("previews");
            std::fs::create_dir_all(&preview_dir).map_err(|e| {
                format!(
                    "Failed to create previews directory '{}': {}",
                    preview_dir.display(),
                    e
                )
            })?;

            let staging_dir = app_cache_dir.join("staging");
            std::fs::create_dir_all(&staging_dir).map_err(|e| {
                format!(
                    "Failed to create staging directory '{}': {}",
                    staging_dir.display(),
                    e
                )
            })?;

            let default_models_dir = std::env::var_os("RESVERA_MODELS_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| app_data_dir.join("models"));

            let settings_path = app_data_dir.join("settings.json");
            let initial_settings = match load_or_migrate_settings(&settings_path) {
                Ok(settings) => settings,
                Err(e) => {
                    tracing::error!(
                        "Failed to load or migrate settings: {e}. Default settings will be used; invalid settings preserved for diagnosis."
                    );
                    AppSettings::default()
                }
            };

            // If the user previously configured a custom models directory, honour it;
            // otherwise fall back to the app-data default.
            let models_dir: PathBuf = initial_settings
                .models_directory
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or(default_models_dir);

            std::fs::create_dir_all(&models_dir).map_err(|e| {
                format!(
                    "Failed to create models directory '{}': {}",
                    models_dir.display(),
                    e
                )
            })?;

            let db = AppDatabase::open(&db_path).map_err(|e| {
                format!(
                    "Failed to initialize job database '{}': {}",
                    db_path.display(),
                    e
                )
            })?;
            db.run_crash_recovery_sweep().map_err(|e| {
                format!("Failed to run database crash recovery sweep: {}", e)
            })?;

            let engine = Arc::new(OrtEngine::new());
            let orchestrator =
                JobOrchestrator::with_models_root(db, engine, preview_dir, &models_dir)
                    .with_staging_dir(&staging_dir);

            if let Ok(swept) = orchestrator.sweep_abandoned_staging() {
                if swept > 0 {
                    tracing::info!(swept, "Swept abandoned staging files at startup");
                }
            }

            let downloader = resvera_models::StagedDownloader::new(&models_dir);
            if let Ok(swept) = downloader.sweep_stale_staging_dirs() {
                if swept > 0 {
                    tracing::info!(swept, "Swept stale model staging directories at startup");
                }
            }

            let settings = Arc::new(Mutex::new(initial_settings));
            let models_root = Arc::new(Mutex::new(models_dir));

            let app_state = AppState {
                orchestrator,
                models_root,
                settings,
                settings_path,
                staging_dir,
                staging_sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
                active_installs: Arc::new(Mutex::new(std::collections::HashMap::new())),
                install_progress: Arc::new(Mutex::new(std::collections::HashMap::new())),
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
            install_model,
            cancel_model_install,
            import_model_file,
            get_model_install_progress,
            pick_model_file,
            uninstall_model,
            create_upscale_job,
            create_batch_jobs,
            cancel_job,
            retry_job,
            pause_queue,
            resume_queue,
            get_queue,
            get_job,
            get_jobs_history,
            list_job_history,
            load_settings,
            save_settings,
            pick_images,
            stage_input_image,
            stage_input_image_base64,
            start_staging_upload,
            append_staging_chunk,
            finish_staging_upload,
            abort_staging_upload,
            minimize_window,
            toggle_maximize_window,
            is_window_maximized,
            close_window,
            read_image_data,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(state) = window.try_state::<AppState>() {
                    shutdown_application_impl(&state, std::time::Duration::from_secs(3));
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Resvera desktop application");
}
