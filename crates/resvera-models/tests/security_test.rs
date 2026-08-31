use resvera_models::{
    ArtifactEntry, CompatibilitySpec, LicenseSpec, ManifestError, ModelManifest, ModelVariant,
    ProvenanceSpec, TensorSpec, TilingSpec,
};

fn base_manifest() -> ModelManifest {
    ModelManifest {
        schema_version: 1,
        id: "realesrgan".into(),
        package_version: "1.0.0".into(),
        display_name: "RealESRGAN".into(),
        family: "rrdb".into(),
        category: "photo".into(),
        description: "description".into(),
        license: LicenseSpec {
            spdx: "MIT".into(),
            upstream_url: "url".into(),
            redistribution_review: "approved".into(),
        },
        provenance: ProvenanceSpec {
            upstream_repository: "repo".into(),
            upstream_revision: "rev".into(),
            source_weight_name: "weight".into(),
            source_weight_sha256: "hash".into(),
            export_recipe: "recipe".into(),
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
            static_shapes_required: true,
        },
        compatibility: CompatibilitySpec {
            engine: "onnx-runtime".into(),
            minimum_engine_version: "1.29.0".into(),
            validated_providers: vec!["cpu".into()],
            validated_precisions: vec!["fp32".into()],
        },
        artifacts: vec![ArtifactEntry {
            path: "artifacts/model.onnx".into(),
            size_bytes: 100,
            sha256: "0".repeat(64),
        }],
    }
}

#[test]
fn test_manifest_path_traversal_detection() {
    // Valid manifest passes
    let valid = base_manifest();
    assert!(valid.validate().is_ok());

    // 1. Artifact path traversal with ../
    let mut bad_artifact = base_manifest();
    bad_artifact.artifacts[0].path = "../../../windows/system32/cmd.exe".into();
    assert!(matches!(
        bad_artifact.validate(),
        Err(ManifestError::PathTraversal(_))
    ));

    // 2. Variant artifact absolute path
    let mut bad_variant = base_manifest();
    bad_variant.variants[0].artifact = "/etc/shadow".into();
    assert!(matches!(
        bad_variant.validate(),
        Err(ManifestError::PathTraversal(_))
    ));

    // 3. Windows drive letter traversal
    let mut bad_drive = base_manifest();
    bad_drive.artifacts[0].path = "C:\\evil.dll".into();
    assert!(matches!(
        bad_drive.validate(),
        Err(ManifestError::PathTraversal(_))
    ));
}
