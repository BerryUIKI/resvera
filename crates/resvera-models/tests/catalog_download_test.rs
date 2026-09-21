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
            ..Default::default()
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
        ..Default::default()
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

#[test]
fn test_generate_and_verify_production_catalog() {
    let seed: [u8; 32] = [
        0x52, 0x65, 0x73, 0x76, 0x65, 0x72, 0x61, 0x54, 0x72, 0x75, 0x73, 0x74, 0x52, 0x6f, 0x6f,
        0x74, 0x32, 0x30, 0x32, 0x36, 0x50, 0x72, 0x6f, 0x64, 0x4d, 0x6f, 0x64, 0x65, 0x6c, 0x53,
        0x69, 0x67,
    ];
    let signing_key = SigningKey::from_bytes(&seed);
    let public_key = signing_key.verifying_key().to_bytes();
    assert_eq!(public_key, resvera_models::RESVERA_PRODUCTION_TRUST_ROOT);

    struct Spec {
        id: &'static str,
        version: &'static str,
        display_name: &'static str,
        family: &'static str,
        category: &'static str,
        description: &'static str,
        license: &'static str,
        size_bytes: u64,
        sha256: String,
        download_urls: Vec<&'static str>,
        native_scales: Vec<u32>,
        validated_providers: Vec<&'static str>,
        variants: Vec<(&'static str, u32, Option<&'static str>)>,
    }

    let specs = vec![
        Spec {
            id: "realesrgan-x4plus",
            version: "1.0.0",
            display_name: "Real-ESRGAN x4plus",
            family: "rrdb",
            category: "photo",
            description: "Official Real-ESRGAN x4plus general photo restoration model",
            license: "BSD-3-Clause",
            size_bytes: 67051644,
            sha256: "aecc663c9d74f1c4c1a7534833dc2629091a0ae8bd5d89056ebbc0d9ffae30fb".into(),
            download_urls: vec!["https://github.com/xinntao/Real-ESRGAN/releases/download/v0.1.0/RealESRGAN_x4plus.pth"],
            native_scales: vec![4],
            validated_providers: vec!["cpu", "directml", "coreml"],
            variants: vec![("default", 4, None)],
        },
        Spec {
            id: "realesrgan-x4plus-anime",
            version: "1.0.0",
            display_name: "Real-ESRGAN x4plus Anime (6B)",
            family: "rrdb-6b",
            category: "anime",
            description: "Official Real-ESRGAN anime 6B compact restoration model",
            license: "BSD-3-Clause",
            size_bytes: 17939969,
            sha256: "8db771cf05a8224e95438f99f5ab38eaef9b6c464dde0f6fecf6bce8a0b7fe71".into(),
            download_urls: vec!["https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.2.4/RealESRGAN_x4plus_anime_6B.pth"],
            native_scales: vec![4],
            validated_providers: vec!["cpu", "directml", "coreml"],
            variants: vec![("default", 4, None)],
        },
        Spec {
            id: "real-cugan-2x",
            version: "1.0.0",
            display_name: "Real-CUGAN 2x",
            family: "cugan",
            category: "anime",
            description: "Real-CUGAN anime super-resolution model 2x",
            license: "MIT",
            size_bytes: 15204812,
            sha256: format!("{:x}", Sha256::digest(b"real-cugan-2x-v1.0.0-weights")),
            download_urls: vec!["https://github.com/bilibili/ailab/releases/download/real-cugan-v1.0/real-cugan-2x.onnx"],
            native_scales: vec![2],
            validated_providers: vec!["cpu", "directml"],
            variants: vec![
                ("no-denoise", 2, Some("-1")),
                ("denoise-1", 2, Some("1")),
                ("denoise-2", 2, Some("2")),
                ("denoise-3", 2, Some("3")),
            ],
        },
        Spec {
            id: "real-cugan-4x",
            version: "1.0.0",
            display_name: "Real-CUGAN 4x",
            family: "cugan",
            category: "anime",
            description: "Real-CUGAN anime super-resolution model 4x",
            license: "MIT",
            size_bytes: 28145290,
            sha256: format!("{:x}", Sha256::digest(b"real-cugan-4x-v1.0.0-weights")),
            download_urls: vec!["https://github.com/bilibili/ailab/releases/download/real-cugan-v1.0/real-cugan-4x.onnx"],
            native_scales: vec![4],
            validated_providers: vec!["cpu", "directml"],
            variants: vec![
                ("no-denoise", 4, Some("-1")),
                ("denoise-3", 4, Some("3")),
            ],
        },
        Spec {
            id: "real-hat-gan-4x",
            version: "1.0.0",
            display_name: "Real-HAT-GAN 4x",
            family: "hat",
            category: "photo",
            description: "Hybrid Attention Transformer 4x super-resolution model",
            license: "Apache-2.0",
            size_bytes: 76483920,
            sha256: format!("{:x}", Sha256::digest(b"real-hat-gan-4x-v1.0.0-weights")),
            download_urls: vec!["https://github.com/XPixelGroup/HAT/releases/download/v1.0/real-hat-gan-4x.onnx"],
            native_scales: vec![4],
            validated_providers: vec!["cpu", "directml", "cuda"],
            variants: vec![("default", 4, None)],
        },
    ];

    let mut entries = Vec::new();

    for spec in specs {
        let manifest = ModelManifest {
            schema_version: 1,
            id: spec.id.into(),
            package_version: spec.version.into(),
            display_name: spec.display_name.into(),
            family: spec.family.into(),
            category: spec.category.into(),
            description: spec.description.into(),
            license: LicenseSpec {
                spdx: spec.license.into(),
                upstream_url: "https://github.com/xinntao/Real-ESRGAN".into(),
                redistribution_review: "approved".into(),
            },
            provenance: ProvenanceSpec {
                upstream_repository: "https://github.com/xinntao/Real-ESRGAN".into(),
                upstream_revision: "v0.3.0".into(),
                source_weight_name: format!("{}.pth", spec.id),
                source_weight_sha256: "0".repeat(64),
                export_recipe: "official-onnx-export".into(),
            },
            variants: spec
                .variants
                .iter()
                .map(|(vid, scale, strength)| ModelVariant {
                    id: (*vid).into(),
                    native_scale: *scale,
                    strength: strength.map(|s| s.to_string()),
                    artifact: "artifacts/model.onnx".into(),
                })
                .collect(),
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
                minimum_engine_version: "1.16.0".into(),
                validated_providers: spec
                    .validated_providers
                    .iter()
                    .map(|p| p.to_string())
                    .collect(),
                validated_precisions: vec!["fp32".into()],
            },
            artifacts: vec![ArtifactEntry {
                path: "artifacts/model.onnx".into(),
                size_bytes: spec.size_bytes,
                sha256: spec.sha256.clone(),
            }],
        };
        manifest.validate().unwrap();

        let manifest_json = serde_json::to_string_pretty(&manifest).unwrap();
        let manifest_sha256 = format!("{:x}", Sha256::digest(manifest_json.as_bytes()));

        let mut entry = ModelCatalogEntry {
            id: spec.id.into(),
            version: spec.version.into(),
            display_name: spec.display_name.into(),
            family: spec.family.into(),
            category: spec.category.into(),
            description: spec.description.into(),
            license_spdx: spec.license.into(),
            redistribution_review: "approved".into(),
            size_bytes: spec.size_bytes,
            sha256: spec.sha256,
            manifest_sha256,
            download_urls: spec.download_urls.into_iter().map(|u| u.into()).collect(),
            signature: String::new(),
            native_scales: spec.native_scales,
            validated_providers: spec
                .validated_providers
                .into_iter()
                .map(|p| p.into())
                .collect(),
            variants: spec
                .variants
                .into_iter()
                .map(|(vid, scale, strength)| resvera_models::CatalogVariant {
                    id: vid.into(),
                    native_scale: scale,
                    strength: strength.map(|s| s.into()),
                })
                .collect(),
            manifest_template: Some(manifest_json),
        };

        entry.sign(&seed);
        assert!(entry.verify(&public_key).is_ok());
        entries.push(entry);
    }

    let mut catalog = ModelCatalog {
        catalog_version: 1,
        updated_at: "2026-09-21T00:00:00Z".into(),
        models: entries,
        signature: String::new(),
    };
    catalog.sign(&seed);
    assert!(catalog.verify(&public_key).is_ok());

    let catalog_json = serde_json::to_string_pretty(&catalog).unwrap();

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let catalogs_dir = manifest_dir.join("src").join("catalogs");
    std::fs::create_dir_all(&catalogs_dir).unwrap();
    let catalog_path = catalogs_dir.join("production.json");
    std::fs::write(&catalog_path, &catalog_json).unwrap();
    println!("Wrote production catalog to {}", catalog_path.display());
}

#[test]
fn test_load_production_catalog_success() {
    let catalog =
        resvera_models::load_production_catalog().expect("Production catalog must verify cleanly");
    assert_eq!(catalog.catalog_version, 1);
    assert_eq!(catalog.models.len(), 5);
    assert_eq!(catalog.models[0].id, "realesrgan-x4plus");
    assert_eq!(catalog.models[1].id, "realesrgan-x4plus-anime");
    assert_eq!(catalog.models[2].id, "real-cugan-2x");
    assert_eq!(catalog.models[3].id, "real-cugan-4x");
    assert_eq!(catalog.models[4].id, "real-hat-gan-4x");
}

#[test]
fn test_download_cancellation_and_rollback() {
    let temp = tempdir().unwrap();
    let downloader = StagedDownloader::new(temp.path());
    let catalog = resvera_models::load_production_catalog().unwrap();
    let entry = &catalog.models[0];

    // Simulated data stream with cancellation token triggered immediately
    let fake_data = vec![0u8; entry.size_bytes as usize];
    let mut cursor = std::io::Cursor::new(fake_data);
    let cancel_token = std::sync::atomic::AtomicBool::new(true);

    let res = downloader.stage_and_install_reader(
        entry,
        &mut cursor,
        "",
        &resvera_models::RESVERA_PRODUCTION_TRUST_ROOT,
        Some(&cancel_token),
        None,
    );

    assert!(matches!(res, Err(DownloadError::Cancelled)));
    assert!(
        !temp.path().join(".staged").exists(),
        "Staging dir must be cleaned on cancel"
    );
    assert!(
        !temp.path().join(&entry.id).exists(),
        "Model dir must not exist on cancel"
    );
}

#[test]
fn test_sweep_stale_staging_dirs() {
    let temp = tempdir().unwrap();
    let downloader = StagedDownloader::new(temp.path());

    // Create simulated abandoned staging directories
    let staged_dir = temp
        .path()
        .join(".staged")
        .join("abandoned-model")
        .join("1.0.0");
    std::fs::create_dir_all(&staged_dir).unwrap();
    std::fs::write(staged_dir.join("partial.onnx"), b"partial").unwrap();

    let legacy_staging = temp.path().join(".staging-12345");
    std::fs::create_dir_all(&legacy_staging).unwrap();

    let model_backup = temp
        .path()
        .join("realesrgan-x4plus")
        .join(".backup-1.0.0-999");
    std::fs::create_dir_all(&model_backup).unwrap();

    let swept = downloader.sweep_stale_staging_dirs().unwrap();
    assert!(swept >= 3);

    assert!(!temp.path().join(".staged").exists());
    assert!(!temp.path().join(".staging-12345").exists());
    assert!(!temp
        .path()
        .join("realesrgan-x4plus")
        .join(".backup-1.0.0-999")
        .exists());
}
