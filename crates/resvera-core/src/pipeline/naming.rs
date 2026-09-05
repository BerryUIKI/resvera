use chrono::Utc;

pub fn sanitize_filename_component(input: &str) -> String {
    let mut clean = input
        .replace(['/', '\\', '\0', ':', '*', '?', '"', '<', '>', '|'], "_")
        .replace("..", "_");

    // Remove control characters
    clean.retain(|c| !c.is_control());

    let trimmed = clean.trim();
    if trimmed.is_empty() {
        return "output".to_string();
    }

    // Guard against Windows DOS reserved device names
    let upper = trimmed.to_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];

    if reserved.contains(&upper.as_str()) {
        format!("{}_file", trimmed)
    } else {
        // Enforce maximum single filename length
        if trimmed.len() > 200 {
            trimmed[..200].to_string()
        } else {
            trimmed.to_string()
        }
    }
}

pub fn format_output_filename(
    template: &str,
    input_stem: &str,
    model_id: &str,
    scale: u32,
    ext: &str,
) -> String {
    let now = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let effective_template = if template.trim().is_empty() {
        "{stem}_{model}_{scale}x"
    } else {
        template
    };

    let safe_stem = sanitize_filename_component(input_stem);
    let safe_model = sanitize_filename_component(model_id);

    let formatted = effective_template
        .replace("{stem}", &safe_stem)
        .replace("{model}", &safe_model)
        .replace("{scale}", &scale.to_string())
        .replace("{timestamp}", &now);

    let safe_formatted = sanitize_filename_component(&formatted);
    let safe_ext = sanitize_filename_component(ext.trim_start_matches('.'));

    format!("{}.{}", safe_formatted, safe_ext)
}
