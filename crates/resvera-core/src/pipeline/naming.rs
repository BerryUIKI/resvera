use chrono::Utc;

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

    let formatted = effective_template
        .replace("{stem}", input_stem)
        .replace("{model}", model_id)
        .replace("{scale}", &scale.to_string())
        .replace("{timestamp}", &now);

    format!("{}.{}", formatted, ext.trim_start_matches('.'))
}
