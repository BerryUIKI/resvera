use crate::signing::{verify_signature_hex, SigningError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub version: String,
    pub display_name: String,
    pub family: String,
    pub category: String,
    pub description: String,
    pub license_spdx: String,
    pub redistribution_review: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub download_urls: Vec<String>,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCatalog {
    pub catalog_version: u32,
    pub updated_at: String,
    pub models: Vec<ModelCatalogEntry>,
    pub signature: String,
}

impl ModelCatalog {
    /// Computes canonical signing payload for catalog
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut cloned = self.clone();
        cloned.signature.clear();
        serde_json::to_vec(&cloned).unwrap_or_default()
    }

    /// Verifies Ed25519 signature of the entire catalog
    pub fn verify(&self, public_key: &[u8; 32]) -> Result<(), SigningError> {
        let payload = self.signing_payload();
        verify_signature_hex(public_key, &payload, &self.signature)
    }
}
