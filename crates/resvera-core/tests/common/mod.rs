use resvera_core::{
    CancellationToken, EngineCapabilities, EngineError, EngineHealth, EngineId, InferenceEngine,
    ModelSession, OwnedTensor, TensorView,
};
use resvera_models::{
    compute_file_sha256, ArtifactEntry, CompatibilitySpec, LicenseSpec, ModelInstaller,
    ModelManifest, ModelVariant, ProvenanceSpec, TensorSpec, TilingSpec,
};
use std::any::Any;
use std::path::Path;

pub struct MockEngine;

#[allow(dead_code)]
pub fn install_mock_model(models_root: &Path, model_id: &str, native_scale: u32) {
    let stage_dir = models_root.join(format!(".test-stage-{model_id}"));
    let artifacts_dir = stage_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir).unwrap();
    let artifact_path = artifacts_dir.join("model.onnx");
    let model_bytes = if native_scale == 2 {
        b"verified-2x-model".as_slice()
    } else {
        b"verified-4x-model".as_slice()
    };
    std::fs::write(&artifact_path, model_bytes).unwrap();
    let artifact_hash = compute_file_sha256(&artifact_path).unwrap();
    let manifest = ModelManifest {
        schema_version: 1,
        id: model_id.into(),
        package_version: "1.0.0".into(),
        display_name: "Test model".into(),
        family: "rrdb".into(),
        category: "test".into(),
        description: "Deterministic integration-test model".into(),
        license: LicenseSpec {
            spdx: "MIT".into(),
            upstream_url: "https://example.invalid/test-model".into(),
            redistribution_review: "test-only".into(),
        },
        provenance: ProvenanceSpec {
            upstream_repository: "https://example.invalid/test-model".into(),
            upstream_revision: "test".into(),
            source_weight_name: "test".into(),
            source_weight_sha256: "0".repeat(64),
            export_recipe: "test".into(),
        },
        variants: vec![ModelVariant {
            id: "default".into(),
            native_scale,
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
            minimum: 1,
            recommended: 16,
            overlap: 0,
            window_size: None,
            static_shapes_required: false,
        },
        compatibility: CompatibilitySpec {
            engine: "onnx-runtime".into(),
            minimum_engine_version: "1.28.0".into(),
            validated_providers: vec!["cpu".into()],
            validated_precisions: vec!["fp32".into()],
        },
        artifacts: vec![ArtifactEntry {
            path: "artifacts/model.onnx".into(),
            size_bytes: model_bytes.len() as u64,
            sha256: artifact_hash,
        }],
    };
    std::fs::write(
        stage_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    ModelInstaller::new(models_root)
        .install_package(&stage_dir)
        .unwrap();
}

struct MockSession {
    scale: usize,
}

impl ModelSession for MockSession {
    fn input_shape(&self) -> Option<[usize; 4]> {
        None
    }

    fn output_shape(&self) -> Option<[usize; 4]> {
        None
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl InferenceEngine for MockEngine {
    fn id(&self) -> EngineId {
        EngineId("test-mock".into())
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            engine_id: self.id(),
            supported_providers: vec!["cpu".into()],
            supports_fp16: false,
            supports_dynamic_shapes: true,
        }
    }

    fn probe(&self) -> Result<EngineHealth, EngineError> {
        Ok(EngineHealth {
            healthy: true,
            active_provider: "cpu".into(),
            diagnostic_message: Some("deterministic test engine".into()),
        })
    }

    fn load(
        &self,
        model_bytes: &[u8],
        _provider_preference: Option<&str>,
    ) -> Result<Box<dyn ModelSession>, EngineError> {
        if model_bytes.is_empty() {
            return Err(EngineError::SessionLoad("empty test model".into()));
        }
        let scale = if model_bytes.windows(2).any(|window| window == b"2x")
            || model_bytes == b"pass2_bytes"
        {
            2
        } else {
            4
        };
        Ok(Box::new(MockSession { scale }))
    }

    fn run(
        &self,
        session: &mut dyn ModelSession,
        input: TensorView<'_>,
        cancel: &CancellationToken,
    ) -> Result<OwnedTensor, EngineError> {
        cancel.check()?;
        let session = session
            .as_any_mut()
            .downcast_mut::<MockSession>()
            .ok_or_else(|| EngineError::SessionLoad("foreign test session".into()))?;
        let [batch, channels, height, width] = input.shape;
        let expected_len = batch
            .checked_mul(channels)
            .and_then(|value| value.checked_mul(height))
            .and_then(|value| value.checked_mul(width))
            .ok_or_else(|| EngineError::InvalidTensor("input shape overflow".into()))?;
        if input.data.len() != expected_len {
            return Err(EngineError::InvalidTensor("input length mismatch".into()));
        }

        let output_height = height
            .checked_mul(session.scale)
            .ok_or_else(|| EngineError::InvalidTensor("output height overflow".into()))?;
        let output_width = width
            .checked_mul(session.scale)
            .ok_or_else(|| EngineError::InvalidTensor("output width overflow".into()))?;
        let mut output = vec![0.0; batch * channels * output_height * output_width];
        let input_plane = height * width;
        let output_plane = output_height * output_width;
        for batch_index in 0..batch {
            for channel in 0..channels {
                let input_offset = (batch_index * channels + channel) * input_plane;
                let output_offset = (batch_index * channels + channel) * output_plane;
                for y in 0..output_height {
                    for x in 0..output_width {
                        output[output_offset + y * output_width + x] = input.data
                            [input_offset + (y / session.scale) * width + x / session.scale];
                    }
                }
            }
        }
        OwnedTensor::new([batch, channels, output_height, output_width], output)
    }
}
