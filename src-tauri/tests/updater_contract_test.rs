use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdaterPlatformPayload {
    pub signature: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TauriUpdaterManifest {
    pub version: String,
    pub notes: Option<String>,
    pub pub_date: String,
    pub platforms: HashMap<String, UpdaterPlatformPayload>,
}

impl TauriUpdaterManifest {
    pub fn is_update_eligible(&self, current_version: &str) -> bool {
        let clean_cur = current_version.trim().trim_start_matches('v');
        let clean_target = self.version.trim().trim_start_matches('v');

        // Parse semver parts
        let cur_parts: Vec<u32> = clean_cur
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect();
        let target_parts: Vec<u32> = clean_target
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect();

        if cur_parts.len() < 3 || target_parts.len() < 3 {
            return false;
        }

        target_parts > cur_parts
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version.trim().is_empty() {
            return Err("Manifest version cannot be empty".into());
        }
        if self.pub_date.trim().is_empty() {
            return Err("Manifest pub_date cannot be empty".into());
        }
        if self.platforms.is_empty() {
            return Err("Manifest must specify at least one platform target".into());
        }
        for (platform, payload) in &self.platforms {
            if payload.url.trim().is_empty() {
                return Err(format!("Platform '{}' has empty download URL", platform));
            }
            if payload.signature.trim().is_empty() {
                return Err(format!(
                    "Platform '{}' has empty cryptographic signature",
                    platform
                ));
            }
        }
        Ok(())
    }
}

#[test]
fn test_updater_manifest_parsing_and_validation() {
    let raw_json = r#"{
        "version": "v0.2.0",
        "notes": "Bug fixes and performance improvements",
        "pub_date": "2026-09-22T00:00:00Z",
        "platforms": {
            "windows-x86_64": {
                "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZw==",
                "url": "https://github.com/BerryUIKI/resvera/releases/download/v0.2.0/Resvera_0.2.0_x64-setup.nsis.zip"
            },
            "darwin-aarch64": {
                "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZw==",
                "url": "https://github.com/BerryUIKI/resvera/releases/download/v0.2.0/Resvera_0.2.0_aarch64.app.tar.gz"
            }
        }
    }"#;

    let manifest: TauriUpdaterManifest =
        serde_json::from_str(raw_json).expect("valid manifest JSON");
    assert_eq!(manifest.version, "v0.2.0");
    assert!(manifest.validate().is_ok());
    assert_eq!(manifest.platforms.len(), 2);
}

#[test]
fn test_updater_rejects_downgrade_and_same_version() {
    let mut manifest = TauriUpdaterManifest {
        version: "0.1.0".into(),
        notes: Some("Release 0.1.0".into()),
        pub_date: "2026-09-22T00:00:00Z".into(),
        platforms: HashMap::new(),
    };

    // Equal version must NOT trigger update
    assert!(!manifest.is_update_eligible("0.1.0"));
    assert!(!manifest.is_update_eligible("v0.1.0"));

    // Older version must NOT trigger update (downgrade prevention)
    manifest.version = "v0.0.9".into();
    assert!(!manifest.is_update_eligible("0.1.0"));

    // Newer version MUST trigger update
    manifest.version = "v0.1.1".into();
    assert!(manifest.is_update_eligible("0.1.0"));

    manifest.version = "v0.2.0".into();
    assert!(manifest.is_update_eligible("0.1.0"));

    manifest.version = "v1.0.0".into();
    assert!(manifest.is_update_eligible("0.1.0"));
}

#[test]
fn test_updater_manifest_fails_closed_on_missing_signatures() {
    let mut platforms = HashMap::new();
    platforms.insert(
        "windows-x86_64".into(),
        UpdaterPlatformPayload {
            signature: "".into(), // empty signature!
            url: "https://example.com/app.zip".into(),
        },
    );

    let manifest = TauriUpdaterManifest {
        version: "0.2.0".into(),
        notes: None,
        pub_date: "2026-09-22T00:00:00Z".into(),
        platforms,
    };

    let res = manifest.validate();
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("empty cryptographic signature"));
}

#[test]
fn test_updater_manifest_fails_closed_on_empty_platforms() {
    let manifest = TauriUpdaterManifest {
        version: "0.2.0".into(),
        notes: None,
        pub_date: "2026-09-22T00:00:00Z".into(),
        platforms: HashMap::new(),
    };

    let res = manifest.validate();
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("at least one platform"));
}
