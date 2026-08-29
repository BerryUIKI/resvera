use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "policy", rename_all = "camelCase")]
pub enum MetadataPolicy {
    Strip,
    PreserveSafe { preserve_gps: bool },
}

impl Default for MetadataPolicy {
    fn default() -> Self {
        MetadataPolicy::PreserveSafe {
            preserve_gps: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SanitizedMetadata {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub date_time: Option<String>,
    pub color_space: Option<String>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub width: u32,
    pub height: u32,
}

impl SanitizedMetadata {
    pub fn apply_policy(&mut self, policy: &MetadataPolicy) {
        match policy {
            MetadataPolicy::Strip => {
                self.camera_make = None;
                self.camera_model = None;
                self.date_time = None;
                self.color_space = None;
                self.gps_latitude = None;
                self.gps_longitude = None;
            }
            MetadataPolicy::PreserveSafe { preserve_gps } => {
                if !preserve_gps {
                    self.gps_latitude = None;
                    self.gps_longitude = None;
                }
            }
        }
    }
}
