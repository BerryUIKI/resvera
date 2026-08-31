pub mod catalog;
pub mod downloader;
pub mod installer;
pub mod manifest;
pub mod runtime;
pub mod signing;

pub use catalog::*;
pub use downloader::*;
pub use installer::*;
pub use manifest::*;
pub use runtime::*;
pub use signing::*;

pub fn validate_path_component(value: &str, field_name: &str) -> Result<(), String> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return Err(format!("Unsafe {} value: '{}'", field_name, value));
    }
    Ok(())
}
