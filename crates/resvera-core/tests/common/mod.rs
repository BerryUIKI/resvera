use resvera_core::{
    CancellationToken, EngineCapabilities, EngineError, EngineHealth, EngineId, InferenceEngine,
    ModelSession, OwnedTensor, TensorView,
};
use std::any::Any;

pub struct MockEngine;

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
