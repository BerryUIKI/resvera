use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPolicy {
    Strip,
    PreserveSafe { preserve_gps: bool },
    PreserveAll,
}

impl Default for MetadataPolicy {
    fn default() -> Self {
        MetadataPolicy::PreserveSafe {
            preserve_gps: false,
        }
    }
}

impl std::str::FromStr for MetadataPolicy {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "preserveSafe" | "preserve_safe" => Ok(MetadataPolicy::PreserveSafe {
                preserve_gps: false,
            }),
            "stripAll" | "strip_all" | "strip" => Ok(MetadataPolicy::Strip),
            "preserveAll" | "preserve_all" => Ok(MetadataPolicy::PreserveAll),
            other => Err(format!("Unknown metadata policy: {other}")),
        }
    }
}

impl<'de> Deserialize<'de> for MetadataPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Str(String),
            Obj {
                policy: String,
                #[serde(default, rename = "preserveGps", alias = "preserve_gps")]
                preserve_gps: bool,
            },
        }

        match Helper::deserialize(deserializer)? {
            Helper::Str(s) => match s.as_str() {
                "preserveSafe" | "preserve_safe" => Ok(MetadataPolicy::PreserveSafe {
                    preserve_gps: false,
                }),
                "stripAll" | "strip_all" | "strip" => Ok(MetadataPolicy::Strip),
                "preserveAll" | "preserve_all" => Ok(MetadataPolicy::PreserveAll),
                other => Err(serde::de::Error::custom(format!(
                    "unknown metadata policy string: {other}"
                ))),
            },
            Helper::Obj {
                policy,
                preserve_gps,
            } => match policy.as_str() {
                "preserveSafe" | "preserve_safe" => {
                    Ok(MetadataPolicy::PreserveSafe { preserve_gps })
                }
                "stripAll" | "strip_all" | "strip" => Ok(MetadataPolicy::Strip),
                "preserveAll" | "preserve_all" => Ok(MetadataPolicy::PreserveAll),
                other => Err(serde::de::Error::custom(format!(
                    "unknown metadata policy in object: {other}"
                ))),
            },
        }
    }
}

impl Serialize for MetadataPolicy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            MetadataPolicy::Strip => serializer.serialize_str("stripAll"),
            MetadataPolicy::PreserveSafe {
                preserve_gps: false,
            } => serializer.serialize_str("preserveSafe"),
            MetadataPolicy::PreserveSafe { preserve_gps: true } => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("policy", "preserveSafe")?;
                map.serialize_entry("preserveGps", &true)?;
                map.end()
            }
            MetadataPolicy::PreserveAll => serializer.serialize_str("preserveAll"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RawImageMetadata {
    pub icc_profile: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
    pub orientation_was_applied: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SanitizedOutputMetadata {
    pub icc_profile: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

impl RawImageMetadata {
    pub fn sanitize_for_output(&self, policy: &MetadataPolicy) -> SanitizedOutputMetadata {
        match policy {
            MetadataPolicy::Strip => SanitizedOutputMetadata {
                icc_profile: None,
                exif: None,
                xmp: None,
            },
            MetadataPolicy::PreserveSafe { preserve_gps } => {
                let exif = self.exif.as_ref().map(|raw_exif| {
                    sanitize_exif(raw_exif, *preserve_gps, self.orientation_was_applied)
                });
                let xmp = if *preserve_gps {
                    self.xmp.clone()
                } else {
                    self.xmp.as_ref().map(|x| strip_gps_from_xmp(x))
                };
                SanitizedOutputMetadata {
                    icc_profile: self.icc_profile.clone(),
                    exif,
                    xmp,
                }
            }
            MetadataPolicy::PreserveAll => {
                let exif = self
                    .exif
                    .as_ref()
                    .map(|raw_exif| sanitize_exif(raw_exif, true, self.orientation_was_applied));
                SanitizedOutputMetadata {
                    icc_profile: self.icc_profile.clone(),
                    exif,
                    xmp: self.xmp.clone(),
                }
            }
        }
    }
}

/// Modifies the raw EXIF/TIFF payload in place:
/// 1. If `reset_orientation` is true, tag 0x0112 in IFD0 is set to 1 (Normal) so image viewers
///    do not apply rotation a second time after pixel processing.
/// 2. If `preserve_gps` is false, tag 0x8825 (GPS IFD pointer) is removed and GPS table data is wiped.
pub fn sanitize_exif(raw_exif: &[u8], preserve_gps: bool, reset_orientation: bool) -> Vec<u8> {
    let mut data = raw_exif.to_vec();
    if data.len() < 8 {
        return data;
    }

    let tiff_offset = if data.starts_with(b"Exif\0\0") { 6 } else { 0 };

    if data.len() < tiff_offset + 8 {
        return data;
    }

    let tiff = &mut data[tiff_offset..];
    let is_le = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return data,
    };

    let magic = if is_le {
        u16::from_le_bytes([tiff[2], tiff[3]])
    } else {
        u16::from_be_bytes([tiff[2], tiff[3]])
    };
    if magic != 42 {
        return data;
    }

    let ifd0_offset = if is_le {
        u32::from_le_bytes([tiff[4], tiff[5], tiff[6], tiff[7]]) as usize
    } else {
        u32::from_be_bytes([tiff[4], tiff[5], tiff[6], tiff[7]]) as usize
    };

    if ifd0_offset + 2 > tiff.len() {
        return data;
    }

    let num_entries = if is_le {
        u16::from_le_bytes([tiff[ifd0_offset], tiff[ifd0_offset + 1]]) as usize
    } else {
        u16::from_be_bytes([tiff[ifd0_offset], tiff[ifd0_offset + 1]]) as usize
    };

    let mut current_offset = ifd0_offset + 2;
    for _ in 0..num_entries {
        if current_offset + 12 > tiff.len() {
            break;
        }

        let tag = if is_le {
            u16::from_le_bytes([tiff[current_offset], tiff[current_offset + 1]])
        } else {
            u16::from_be_bytes([tiff[current_offset], tiff[current_offset + 1]])
        };

        // Tag 0x0112: Orientation (SHORT, 2 bytes)
        if tag == 0x0112 && reset_orientation {
            if is_le {
                tiff[current_offset + 8] = 0x01;
                tiff[current_offset + 9] = 0x00;
                tiff[current_offset + 10] = 0x00;
                tiff[current_offset + 11] = 0x00;
            } else {
                tiff[current_offset + 8] = 0x00;
                tiff[current_offset + 9] = 0x01;
                tiff[current_offset + 10] = 0x00;
                tiff[current_offset + 11] = 0x00;
            }
        }

        // Tag 0x8825: GPSInfo IFD pointer
        if tag == 0x8825 && !preserve_gps {
            let gps_ifd_offset = if is_le {
                u32::from_le_bytes([
                    tiff[current_offset + 8],
                    tiff[current_offset + 9],
                    tiff[current_offset + 10],
                    tiff[current_offset + 11],
                ]) as usize
            } else {
                u32::from_be_bytes([
                    tiff[current_offset + 8],
                    tiff[current_offset + 9],
                    tiff[current_offset + 10],
                    tiff[current_offset + 11],
                ]) as usize
            };

            // Wipe GPS sub-IFD entries and data if within bounds
            if gps_ifd_offset + 2 <= tiff.len() {
                let gps_entries = if is_le {
                    u16::from_le_bytes([tiff[gps_ifd_offset], tiff[gps_ifd_offset + 1]]) as usize
                } else {
                    u16::from_be_bytes([tiff[gps_ifd_offset], tiff[gps_ifd_offset + 1]]) as usize
                };
                let gps_table_len = 2 + gps_entries * 12 + 4;
                let end = (gps_ifd_offset + gps_table_len).min(tiff.len());
                for b in &mut tiff[gps_ifd_offset..end] {
                    *b = 0;
                }
            }

            // Invalidate the GPS tag in IFD0 so readers skip it
            tiff[current_offset] = 0x00;
            tiff[current_offset + 1] = 0x00;
            tiff[current_offset + 8] = 0x00;
            tiff[current_offset + 9] = 0x00;
            tiff[current_offset + 10] = 0x00;
            tiff[current_offset + 11] = 0x00;
        }

        current_offset += 12;
    }

    data
}

/// Strips GPS elements and attributes from an XMP string buffer.
pub fn strip_gps_from_xmp(raw_xmp: &[u8]) -> Vec<u8> {
    let Ok(xmp_str) = std::str::from_utf8(raw_xmp) else {
        return raw_xmp.to_vec();
    };

    let re_tag = regex::Regex::new(r"(?is)<[^>]*gps[^>]*>.*?</[^>]*gps[^>]*>").unwrap();
    let re_self_closing = regex::Regex::new(r"(?is)<[^>]*gps[^>]*/>").unwrap();
    let re_attr = regex::Regex::new(r#"(?i)\s*exif:GPS\w+="[^"]*""#).unwrap();

    let cleaned = re_tag.replace_all(xmp_str, "");
    let cleaned = re_self_closing.replace_all(&cleaned, "");
    let cleaned = re_attr.replace_all(&cleaned, "");

    cleaned.into_owned().into_bytes()
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
            MetadataPolicy::PreserveAll => {}
        }
    }
}
