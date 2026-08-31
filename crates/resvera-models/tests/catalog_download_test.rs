use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use resvera_models::{
    sign_payload, ArtifactEntry, CompatibilitySpec, DownloadError, LicenseSpec, ModelCatalog,
    ModelCatalogEntry, ModelInstaller, ModelManifest, ModelVariant, ProvenanceSpec,
    StagedDownloader, TensorSpec, TilingSpec,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[test]
fn test_catalog_signing_and_verification() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let public_key = signing_key.verifying_key().to_bytes();

    let mut catalog = ModelCatalog {
        catalog_version: 1,
        updated_at: "2026-08-29T12:00:00Z".into(),
        models: vec![ModelCatalogEntry {
            id: "realesrgan-x4plus".into(),
            version: "1.0.0".into(),
            display_name: "Real-ESRGAN x4plus".into(),
            family: "rrdb".into(),
            category: "photo".into(),
            description: "Official RRDB 4x model".into(),
            license_spdx: "BSD-3-Clause".into(),
            redistribution_review: "approved".into(),
            size_bytes: 1024,
            sha256: "abc123hash".into(),
            manifest_sha256: "manifest-hash".into(),
            download_urls: vec!["https://models.resvera.local/realesrgan-x4plus.pkg".into()],
            signature: "dummy_sig".into(),
        }],
        signature: String::new(),
    };

    let payload = catalog.signing_payload();
    let signature = sign_payload(&payload, &signing_key.to_bytes());
    catalog.signature = signature;

    // Verify catalog
    assert!(catalog.verify(&public_key).is_ok());

    // Tamper catalog
    catalog.models[0].display_name = "Tampered Name".into();
    assert!(catalog.verify(&public_key).is_err());
}

#[test]
fn test_staged_download_and_hash_enforcement() {
    let temp = tempdir().unwrap();
    let downloader = StagedDownloader::new(temp.path());
    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key = signing_key.verifying_key().to_bytes();

    let fake_weights = b"simulated high quality onnx weights 123456789";
    let mut hasher = Sha256::new();
    hasher.update(fake_weights);
    let true_sha256 = format!("{:x}", hasher.finalize());

    let manifest = ModelManifest {
        schema_version: 1,
        id: "realesrgan-x4plus".into(),
        package_version: "1.0.0".into(),
        display_name: "Real-ESRGAN x4plus".into(),
        family: "rrdb".into(),
        category: "photo".into(),
        description: "Official RRDB 4x model".into(),
        license: LicenseSpec {
            spdx: "BSD-3-Clause".into(),
            upstream_url: "https://example.com".into(),
            redistribution_review: "approved".into(),
        },
        provenance: ProvenanceSpec {
            upstream_repository: "https://example.com".into(),
            upstream_revision: "abcdef".into(),
            source_weight_name: "model.pth".into(),
            source_weight_sha256: "1234".into(),
            export_recipe: "recipe.toml".into(),
        },
        variants: vec![ModelVariant {
            id: "default".into(),
            native_scale: 4,
            strength: None,
            artifact: "artifacts/model.onnx".into(),
        }],
        tensor: TensorSpec {
            input_name: "input".into(),
            output_name: "output".into(),
            layout: "NCHW".into(),
            channels: "RGB".into(),
            input_range: [0.0, 1.0],
            output_range: [0.0, 1.0],
            element_type: "float32".into(),
        },
        tiling: TilingSpec {
            alignment: 1,
            minimum: 32,
            recommended: 256,
            overlap: 16,
            window_size: None,
            static_shapes_required: false,
        },
        compatibility: CompatibilitySpec {
            engine: "onnx-runtime".into(),
            minimum_engine_version: "1.16".into(),
            validated_providers: vec!["cpu".into()],
            validated_precisions: vec!["fp32".into()],
        },
        artifacts: vec![ArtifactEntry {
            path: "artifacts/model.onnx".into(),
            size_bytes: fake_weights.len() as u64,
            sha256: true_sha256.clone(),
        }],
    };

    let signed_manifest_json = serde_json::to_string_pretty(&manifest).unwrap();
    let manifest_sha256 = format!("{:x}", Sha256::digest(signed_manifest_json.as_bytes()));

    let mut entry = ModelCatalogEntry {
        id: "realesrgan-x4plus".into(),
        version: "1.0.0".into(),
        display_name: "Real-ESRGAN x4plus".into(),
        family: "rrdb".into(),
        category: "photo".into(),
        description: "Official RRDB 4x model".into(),
        license_spdx: "BSD-3-Clause".into(),
        redistribution_review: "approved".into(),
        size_bytes: fake_weights.len() as u64,
        sha256: true_sha256.clone(),
        manifest_sha256,
        download_urls: vec!["https://local/pkg".into()],
        signature: String::new(),
    };
    entry.signature = sign_payload(&entry.signing_payload(), &signing_key.to_bytes());

    // 1. Success case: data matches sha256
    let chunks: Vec<&[u8]> = vec![&fake_weights[0..10], &fake_weights[10..]];
    let installed_dir = downloader
        .stage_and_install(&entry, &chunks, &signed_manifest_json, &public_key)
        .unwrap();
    assert!(installed_dir.exists());

    let installer = ModelInstaller::new(temp.path());
    assert_eq!(
        installer.get_active_version("realesrgan-x4plus").unwrap(),
        Some("1.0.0".into())
    );

    // 2. Failure case: corrupted chunk
    let corrupt_chunks: Vec<&[u8]> = vec![b"corrupted bytes"];
    let err =
        downloader.stage_and_install(&entry, &corrupt_chunks, &signed_manifest_json, &public_key);
    assert!(matches!(err, Err(DownloadError::HashMismatch { .. })));

    let tampered_manifest = signed_manifest_json.replace("Official RRDB", "Tampered RRDB");
    let err = downloader.stage_and_install(&entry, &chunks, &tampered_manifest, &public_key);
    assert!(matches!(err, Err(DownloadError::SignatureInvalid(_))));

    // Ensure staging directory was wiped
    assert!(!temp.path().join(".staged").exists());
}
