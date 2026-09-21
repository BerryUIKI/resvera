use resvera_persistence::{
    decode_cursor, encode_cursor, AppDatabase, DatabaseError, JobRecord, MAX_PAGE_SIZE,
    MIN_PAGE_SIZE,
};
use tempfile::tempdir;

fn create_job(id: &str, created_at: &str) -> JobRecord {
    JobRecord {
        id: id.to_string(),
        state: "completed".to_string(),
        input_path: "/test/input.png".to_string(),
        output_path: Some("/test/output.png".to_string()),
        preview_path: None,
        model_id: "realesrgan-x4plus".to_string(),
        model_package_version: "1.0.0".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        engine_id: "ort".to_string(),
        provider_id: Some("cpu".to_string()),
        progress_fraction: 1.0,
        progress_stage: "completed".to_string(),
        error_code: None,
        error_message: None,
        output_directory: None,
        output_format_json: None,
        overwrite: false,
        tile_size: None,
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
    }
}

#[test]
fn test_pagination_deterministic_ordering_and_timestamp_collision() {
    let temp_dir = tempdir().unwrap();
    let db = AppDatabase::open(temp_dir.path().join("test.db")).unwrap();

    // 4 jobs: job-b and job-a have the exact same timestamp
    let j1 = create_job("job-1", "2026-09-20T10:00:00Z");
    let j2 = create_job("job-b", "2026-09-20T12:00:00Z");
    let j3 = create_job("job-a", "2026-09-20T12:00:00Z");
    let j4 = create_job("job-4", "2026-09-20T15:00:00Z");

    db.insert_job(&j1).unwrap();
    db.insert_job(&j2).unwrap();
    db.insert_job(&j3).unwrap();
    db.insert_job(&j4).unwrap();

    let (jobs, cursor) = db.list_jobs_page(10, None).unwrap();
    assert_eq!(jobs.len(), 4);
    assert_eq!(cursor, None);

    // Expected order:
    // 1: job-4 (15:00)
    // 2: job-b (12:00, id "job-b" > "job-a")
    // 3: job-a (12:00, id "job-a")
    // 4: job-1 (10:00)
    assert_eq!(jobs[0].id, "job-4");
    assert_eq!(jobs[1].id, "job-b");
    assert_eq!(jobs[2].id, "job-a");
    assert_eq!(jobs[3].id, "job-1");
}

#[test]
fn test_multi_page_traversal_and_exhaustion() {
    let temp_dir = tempdir().unwrap();
    let db = AppDatabase::open(temp_dir.path().join("test.db")).unwrap();

    // Insert 7 jobs with increasing timestamps
    for i in 1..=7 {
        let job = create_job(&format!("job-{i:02}"), &format!("2026-09-20T10:{i:02}:00Z"));
        db.insert_job(&job).unwrap();
    }

    // Page size 2
    let mut collected_ids = Vec::new();
    let mut current_cursor: Option<String> = None;
    let mut page_count = 0;

    loop {
        let (page, next_cursor) = db.list_jobs_page(2, current_cursor.as_deref()).unwrap();
        page_count += 1;
        for job in page {
            collected_ids.push(job.id);
        }

        if next_cursor.is_none() {
            break;
        }
        current_cursor = next_cursor;
    }

    assert_eq!(page_count, 4); // 2 + 2 + 2 + 1 = 7 jobs across 4 pages
    assert_eq!(collected_ids.len(), 7);
    assert_eq!(
        collected_ids,
        vec!["job-07", "job-06", "job-05", "job-04", "job-03", "job-02", "job-01"]
    );
}

#[test]
fn test_page_boundary_exact_match_no_spurious_cursor() {
    let temp_dir = tempdir().unwrap();
    let db = AppDatabase::open(temp_dir.path().join("test.db")).unwrap();

    // Exactly 4 jobs
    for i in 1..=4 {
        let job = create_job(&format!("job-{i}"), &format!("2026-09-20T10:{i:02}:00Z"));
        db.insert_job(&job).unwrap();
    }

    // Page 1 with limit 2
    let (p1, c1) = db.list_jobs_page(2, None).unwrap();
    assert_eq!(p1.len(), 2);
    assert!(
        c1.is_some(),
        "Next cursor must be present when more records exist"
    );

    // Page 2 with limit 2
    let (p2, c2) = db.list_jobs_page(2, c1.as_deref()).unwrap();
    assert_eq!(p2.len(), 2);
    assert_eq!(
        c2, None,
        "Next cursor must be None when all remaining items fit in the page"
    );
}

#[test]
fn test_page_size_capping_and_bounds() {
    let temp_dir = tempdir().unwrap();
    let db = AppDatabase::open(temp_dir.path().join("test.db")).unwrap();

    // Insert 120 jobs
    for i in 1..=120 {
        let job = create_job(&format!("job-{i:03}"), &format!("2026-09-20T10:{i:03}:00Z"));
        db.insert_job(&job).unwrap();
    }

    // Limit 0 should clamp to MIN_PAGE_SIZE (1)
    let (p_min, c_min) = db.list_jobs_page(0, None).unwrap();
    assert_eq!(p_min.len(), MIN_PAGE_SIZE);
    assert!(c_min.is_some());

    // Limit 200 should clamp to MAX_PAGE_SIZE (100)
    let (p_max, c_max) = db.list_jobs_page(200, None).unwrap();
    assert_eq!(p_max.len(), MAX_PAGE_SIZE);
    assert_eq!(p_max.len(), 100);
    assert!(c_max.is_some());
}

#[test]
fn test_invalid_and_malformed_cursors() {
    let temp_dir = tempdir().unwrap();
    let db = AppDatabase::open(temp_dir.path().join("test.db")).unwrap();

    let j = create_job("job-1", "2026-09-20T10:00:00Z");
    db.insert_job(&j).unwrap();

    // 1. Empty string
    let res = db.list_jobs_page(10, Some(""));
    assert!(matches!(res, Err(DatabaseError::InvalidCursor(_))));

    // 2. Not valid base64
    let res = db.list_jobs_page(10, Some("!not-base64!"));
    assert!(matches!(res, Err(DatabaseError::InvalidCursor(_))));

    // 3. Valid base64 but not json
    use base64::Engine;
    let b64_not_json = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"hello world");
    let res = db.list_jobs_page(10, Some(&b64_not_json));
    assert!(matches!(res, Err(DatabaseError::InvalidCursor(_))));

    // 4. Valid JSON but missing required fields
    let b64_empty_obj = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"{\"t\":\"\"}");
    let res = db.list_jobs_page(10, Some(&b64_empty_obj));
    assert!(matches!(res, Err(DatabaseError::InvalidCursor(_))));

    // 5. Valid cursor decoding
    let valid_cursor = encode_cursor("2026-09-20T10:00:00Z", "job-1");
    let decoded = decode_cursor(&valid_cursor).unwrap();
    assert_eq!(decoded.0, "2026-09-20T10:00:00Z");
    assert_eq!(decoded.1, "job-1");
}

#[test]
fn test_concurrent_deletion_and_insertion_predictability() {
    let temp_dir = tempdir().unwrap();
    let db = AppDatabase::open(temp_dir.path().join("test.db")).unwrap();

    for i in 1..=5 {
        let job = create_job(&format!("job-{i}"), &format!("2026-09-20T10:0{i}:00Z"));
        db.insert_job(&job).unwrap();
    }

    // Fetch page 1 (size 2): gets job-5, job-4
    let (p1, c1) = db.list_jobs_page(2, None).unwrap();
    assert_eq!(p1[0].id, "job-5");
    assert_eq!(p1[1].id, "job-4");
    let cursor = c1.unwrap();

    // Now delete job-3 from database
    let deleted = db.delete_job("job-3").unwrap();
    assert!(deleted);

    // Concurrently insert a NEWER job (created_at in future: 10:10:00Z)
    let new_job = create_job("job-newer", "2026-09-20T10:10:00Z");
    db.insert_job(&new_job).unwrap();

    // Fetch page 2 using cursor from page 1
    // It should seamlessly skip deleted job-3 and return [job-2, job-1],
    // without including job-newer (which was created before page 1's position in descending order).
    let (p2, c2) = db.list_jobs_page(2, Some(&cursor)).unwrap();
    assert_eq!(p2.len(), 2);
    assert_eq!(p2[0].id, "job-2");
    assert_eq!(p2[1].id, "job-1");
    assert_eq!(c2, None);
}
