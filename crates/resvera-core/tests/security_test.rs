use resvera_core::{format_output_filename, sanitize_filename_component};

#[test]
fn test_path_traversal_and_dos_device_name_sanitization() {
    // 1. Path traversal injection
    let attack_stem = "../../../etc/passwd";
    let formatted = format_output_filename("{stem}_{model}_{scale}x", attack_stem, "realesrgan", 4, "png");
    assert!(!formatted.contains(".."));
    assert!(!formatted.contains("/"));
    assert!(!formatted.contains("\\"));

    // 2. Windows DOS reserved names
    let con_stem = "CON";
    let con_sanitized = sanitize_filename_component(con_stem);
    assert_eq!(con_sanitized, "CON_file");

    let nul_stem = "nul";
    let nul_sanitized = sanitize_filename_component(nul_stem);
    assert_eq!(nul_sanitized, "nul_file");

    // 3. Null bytes and control characters
    let null_byte_stem = "photo\0_exploit.exe";
    let safe = sanitize_filename_component(null_byte_stem);
    assert_eq!(safe, "photo__exploit.exe");
    assert!(!safe.contains('\0'));

    // 4. Overly long filename
    let long_stem = "a".repeat(500);
    let safe_long = sanitize_filename_component(&long_stem);
    assert!(safe_long.len() <= 200);
}
