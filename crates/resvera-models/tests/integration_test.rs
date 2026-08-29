use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use resvera_models::{
    compute_file_sha256, sign_message, verify_signature, ArtifactEntry, CompatibilitySpec,
    LicenseSpec, ModelInstaller, ModelManifest, ModelVariant, ProvenanceSpec, SigningError,
    TensorSpec, TilingSpec,
};
use std::fs;
use tempfile::tempdir;

fn sample_manifest(id: &str, version: &str, artifact_name: &str, hash: &str) -> ModelManifest {
    ModelManifest {
        schema_version: 1,
        id: id.into(),
        package_version: version.into(),
        display_name: "Real-ESRGAN x4plus".into(),
        family: "rrdb".into(),
        category: "photo".into(),
        description: "Photo restoration".into(),
        license: LicenseSpec {
            spdx: "BSD-3-Clause".into(),
            upstream_url: "https://github.com/xinntao/Real-ESRGAN".into(),
            redistribution_review: "approved".into(),
        },
        provenance: ProvenanceSpec {
            upstream_repository: "https://github.com/xinntao/Real-ESRGAN".into(),
            upstream_revision: "4a5b6c".into(),
            source_weight_name: "RealESRGAN_x4plus.pth".into(),
            source_weight_sha256: "deadbeef".into(),
            export_recipe: "exports/realesrgan-x4plus/v1.toml".into(),
        },
        variants: vec![ModelVariant {
            id: "default".into(),
            native_scale: 4,
            strength: None,
            artifact: format!("artifacts/{}", artifact_name),
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
            static_shapes_required: true,
        },
        compatibility: CompatibilitySpec {
            engine: "onnx-runtime".into(),
            minimum_engine_version: "1.16.0".into(),
            validated_providers: vec!["cpu".into(), "directml".into()],
            validated_precisions: vec!["fp32".into()],
        },
        artifacts: vec![ArtifactEntry {
            path: format!("artifacts/{}", artifact_name),
            size_bytes: 32,
            sha256: hash.into(),
        }],
    }
}

#[test]
fn test_ed25519_catalog_signature_workflow() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key_bytes = signing_key.verifying_key().to_bytes();

    let catalog_payload = b"{\"schema_version\":1,\"models\":[\"realesrgan-x4plus\"]}";
    let signature = sign_message(&signing_key, catalog_payload);

    // 1. Valid verification
    assert!(verify_signature(&verifying_key_bytes, catalog_payload, &signature).is_ok());

    // 2. Tampered payload
    let tampered_payload = b"{\"schema_version\":1,\"models\":[\"realesrgan-x4plus\",\"malicious-model\"]}";
    assert!(matches!(
        verify_signature(&verifying_key_bytes, tampered_payload, &signature),
        Err(SigningError::VerificationFailed)
    ));
}

#[test]
fn test_package_installation_and_atomic_rollback() {
    let models_dir = tempdir().unwrap();
    let installer = ModelInstaller::new(models_dir.path());

    // --- Version 1.0.0 ---
    let stage1 = tempdir().unwrap();
    let art_dir1 = stage1.path().join("artifacts");
    fs::create_dir_all(&art_dir1).unwrap();
    let art1 = art_dir1.join("model.onnx");
    fs::write(&art1, b"realesrgan model weights v1.0.0").unwrap();
    let hash1 = compute_file_sha256(&art1).unwrap();

    let manifest1 = sample_manifest("realesrgan-x4plus", "1.0.0", "model.onnx", &hash1);
    fs::write(
        stage1.path().join("manifest.json"),
        serde_json::to_string_pretty(&manifest1).unwrap(),
    )
    .unwrap();

    let res1 = installer.install_package(stage1.path());
    assert!(res1.is_ok());
    assert_eq!(
        installer.get_active_version("realesrgan-x4plus").unwrap(),
        Some("1.0.0".to_string())
    );

    // --- Version 1.1.0 ---
    let stage2 = tempdir().unwrap();
    let art_dir2 = stage2.path().join("artifacts");
    fs::create_dir_all(&art_dir2).unwrap();
    let art2 = art_dir2.join("model.onnx");
    fs::write(&art2, b"realesrgan model weights v1.1.0 upgraded").unwrap();
    let hash2 = compute_file_sha256(&art2).unwrap();

    let manifest2 = sample_manifest("realesrgan-x4plus", "1.1.0", "model.onnx", &hash2);
    fs::write(
        stage2.path().join("manifest.json"),
        serde_json::to_string_pretty(&manifest2).unwrap(),
    )
    .unwrap();

    let res2 = installer.install_package(stage2.path());
    assert!(res2.is_ok());
    assert_eq!(
        installer.get_active_version("realesrgan-x4plus").unwrap(),
        Some("1.1.0".to_string())
    );

    // --- Rollback to 1.0.0 ---
    let rb = installer.activate_version("realesrgan-x4plus", "1.0.0");
    assert!(rb.is_ok());
    assert_eq!(
        installer.get_active_version("realesrgan-x4plus").unwrap(),
        Some("1.0.0".to_string())
    );
}
