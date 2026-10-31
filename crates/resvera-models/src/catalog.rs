use crate::signing::{sign_payload, verify_signature_hex, SigningError};
use serde::{Deserialize, Serialize};

/// Pinned Ed25519 public key (trust root) for official Resvera model catalog and package signing.
///
/// Rotated on 2026-11-01 because the previous seed was committed in plaintext in
/// the test suite. The production signing seed is held out-of-tree; see
/// `tools/catalog-sign/README.md` for the rotation and signing procedure.
pub const RESVERA_PRODUCTION_TRUST_ROOT: [u8; 32] = [
    248, 210, 130, 19, 134, 62, 145, 99, 81, 82, 79, 53, 12, 100, 11, 235, 15, 29, 40, 226, 37,
    112, 50, 170, 226, 157, 60, 68, 226, 190, 133, 191,
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CatalogVariant {
    pub id: String,
    pub native_scale: u32,
    #[serde(default)]
    pub strength: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
    pub manifest_sha256: String,
    pub download_urls: Vec<String>,
    pub signature: String,
    #[serde(default)]
    pub native_scales: Vec<u32>,
    #[serde(default)]
    pub validated_providers: Vec<String>,
    #[serde(default)]
    pub variants: Vec<CatalogVariant>,
    #[serde(default)]
    pub manifest_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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

    /// Signs the catalog with the provided 32-byte secret key
    pub fn sign(&mut self, secret_key: &[u8; 32]) {
        self.signature.clear();
        let payload = self.signing_payload();
        self.signature = sign_payload(&payload, secret_key);
    }

    /// Verifies Ed25519 signature of the entire catalog
    pub fn verify(&self, public_key: &[u8; 32]) -> Result<(), SigningError> {
        let payload = self.signing_payload();
        verify_signature_hex(public_key, &payload, &self.signature)
    }
}

impl ModelCatalogEntry {
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut cloned = self.clone();
        cloned.signature.clear();
        serde_json::to_vec(&cloned).unwrap_or_default()
    }

    pub fn sign(&mut self, secret_key: &[u8; 32]) {
        self.signature.clear();
        let payload = self.signing_payload();
        self.signature = sign_payload(&payload, secret_key);
    }

    pub fn verify(&self, public_key: &[u8; 32]) -> Result<(), SigningError> {
        verify_signature_hex(public_key, &self.signing_payload(), &self.signature)
    }
}

/// Loads the official embedded Resvera production model catalog and verifies its signature
/// as well as every contained model entry against the pinned trust root.
pub fn load_production_catalog() -> Result<ModelCatalog, SigningError> {
    let catalog_raw = include_str!("catalogs/production.json");
    let catalog: ModelCatalog =
        serde_json::from_str(catalog_raw).map_err(|_e| SigningError::VerificationFailed)?;
    catalog.verify(&RESVERA_PRODUCTION_TRUST_ROOT)?;
    for model in &catalog.models {
        model.verify(&RESVERA_PRODUCTION_TRUST_ROOT)?;
    }
    Ok(catalog)
}
