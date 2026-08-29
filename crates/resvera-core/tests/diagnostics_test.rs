use resvera_core::{DiagnosticCollector, DiagnosticReport};

#[test]
fn test_diagnostics_path_redaction_and_privacy_guarantees() {
    let raw_logs = vec![
        "Loaded image from C:\\Users\\Alice\\Pictures\\family_vacation.jpg".to_string(),
        "Writing output to /home/bob/projects/super_res/output_001.png".to_string(),
        "Using temp cache at /Users/charlie/Library/Caches/resvera_previews".to_string(),
        "DirectML execution provider initialized successfully on GPU 0".to_string(),
    ];

    let report: DiagnosticReport = DiagnosticCollector::generate_report(
        "ort",
        "1.29.0",
        "directml",
        &["cpu".into(), "directml".into()],
        &raw_logs,
    );

    // 1. Check redaction
    assert_eq!(
        report.sanitized_logs[0],
        "Loaded image from <USER_DIR>\\Pictures\\family_vacation.jpg"
    );
    assert_eq!(
        report.sanitized_logs[1],
        "Writing output to <USER_DIR>/projects/super_res/output_001.png"
    );
    assert_eq!(
        report.sanitized_logs[2],
        "Using temp cache at <USER_DIR>/Library/Caches/resvera_previews"
    );
    assert_eq!(
        report.sanitized_logs[3],
        "DirectML execution provider initialized successfully on GPU 0"
    );

    // 2. Privacy invariant assertions
    assert!(!report.contains_pixel_buffers);
    assert!(!report.contains_thumbnails);
    assert!(!report.contains_exif);

    // 3. Serialization to JSON
    let json = serde_json::to_string_pretty(&report).unwrap();
    assert!(!json.contains("Alice"));
    assert!(!json.contains("bob"));
    assert!(!json.contains("charlie"));
}
