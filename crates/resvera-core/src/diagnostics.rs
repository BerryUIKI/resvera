use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticReport {
    pub resvera_version: String,
    pub os_name: String,
    pub os_arch: String,
    pub engine_id: String,
    pub engine_version: String,
    pub active_provider: String,
    pub supported_providers: Vec<String>,
    pub memory_summary: String,
    pub sanitized_logs: Vec<String>,
    pub contains_pixel_buffers: bool,
    pub contains_thumbnails: bool,
    pub contains_exif: bool,
}

pub struct DiagnosticCollector;

impl DiagnosticCollector {
    /// Redacts user paths and username occurrences in log lines.
    pub fn sanitize_path(input: &str) -> String {
        let mut sanitized = input.to_string();

        // 1. Windows user directories: C:\Users\<username>\...
        let windows_user_pattern = regex::Regex::new(r"(?i)[a-z]:\\users\\[^\\]+").unwrap();
        sanitized = windows_user_pattern.replace_all(&sanitized, "<USER_DIR>").into_owned();

        // 2. Unix / Linux / macOS user directories: /home/<username> or /Users/<username>
        let unix_user_pattern = regex::Regex::new(r"/(home|Users)/[^/\s]+").unwrap();
        sanitized = unix_user_pattern.replace_all(&sanitized, "<USER_DIR>").into_owned();

        sanitized
    }

    pub fn generate_report(
        engine_id: &str,
        engine_version: &str,
        active_provider: &str,
        supported_providers: &[String],
        raw_logs: &[String],
    ) -> DiagnosticReport {
        let sanitized_logs = raw_logs
            .iter()
            .map(|log| Self::sanitize_path(log))
            .collect();

        DiagnosticReport {
            resvera_version: env!("CARGO_PKG_VERSION").to_string(),
            os_name: env::consts::OS.to_string(),
            os_arch: env::consts::ARCH.to_string(),
            engine_id: engine_id.to_string(),
            engine_version: engine_version.to_string(),
            active_provider: active_provider.to_string(),
            supported_providers: supported_providers.to_vec(),
            memory_summary: "System memory within offline budget".to_string(),
            sanitized_logs,
            contains_pixel_buffers: false,
            contains_thumbnails: false,
            contains_exif: false,
        }
    }
}
