use resvera_persistence::{AppDatabase, JobRecord};
use tempfile::tempdir;

fn sample_job(id: &str, state: &str) -> JobRecord {
    let now = chrono::Utc::now().to_rfc3339();
    JobRecord {
        id: id.to_string(),
        state: state.to_string(),
        input_path: "/test/input.png".to_string(),
        output_path: None,
        preview_path: None,
        model_id: "realesrgan-x4plus".to_string(),
        model_package_version: "1.0.0".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        engine_id: "ort".to_string(),
        provider_id: Some("cpu".to_string()),
        progress_fraction: 0.5,
        progress_stage: "inference".to_string(),
        error_code: None,
        error_message: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

#[test]
fn test_file_backed_persistence_and_crash_recovery() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("resvera_test.db");

    // 1. Session 1: Populate jobs in various lifecycle states
    {
        let db = AppDatabase::open(&db_path).unwrap();
        let jobs = vec![
            sample_job("job-queued-1", "queued"),
            sample_job("job-queued-2", "queued"),
            sample_job("job-prep", "preparing"),
            sample_job("job-run", "running"),
            sample_job("job-fin", "finalizing"),
            sample_job("job-succ", "succeeded"),
            sample_job("job-fail", "failed"),
            sample_job("job-cancelled", "cancelled"),
        ];
        db.insert_batch_jobs(&jobs).unwrap();
    } // Connection dropped abruptly (simulating crash / shutdown)

    // 2. Session 2: Reopen database on restart and run recovery sweep
    {
        let db = AppDatabase::open(&db_path).unwrap();
        let recovered_count = db.run_crash_recovery_sweep().unwrap();

        // 3 in-flight jobs should be recovered: job-prep, job-run, job-fin
        assert_eq!(recovered_count, 3);

        // Queued jobs remain queued
        assert_eq!(db.get_job("job-queued-1").unwrap().unwrap().state, "queued");
        assert_eq!(db.get_job("job-queued-2").unwrap().unwrap().state, "queued");

        // In-flight jobs transitioned to interrupted
        assert_eq!(db.get_job("job-prep").unwrap().unwrap().state, "interrupted");
        assert_eq!(db.get_job("job-run").unwrap().unwrap().state, "interrupted");
        assert_eq!(db.get_job("job-fin").unwrap().unwrap().state, "interrupted");

        // Terminal jobs remain untouched
        assert_eq!(db.get_job("job-succ").unwrap().unwrap().state, "succeeded");
        assert_eq!(db.get_job("job-fail").unwrap().unwrap().state, "failed");
        assert_eq!(db.get_job("job-cancelled").unwrap().unwrap().state, "cancelled");
    }
}
