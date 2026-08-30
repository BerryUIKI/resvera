use ort::{
    session::Session,
    value::{TensorElementType, TensorRef, ValueType},
};
use resvera_core::{
    CancellationToken, EngineCapabilities, EngineError, EngineHealth, EngineId, InferenceEngine,
    ModelSession, OwnedTensor, TensorView,
};
use std::any::Any;

const CPU_PROVIDER: &str = "cpu";

pub struct OrtEngine {
    default_provider: String,
}

impl Default for OrtEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OrtEngine {
    pub fn new() -> Self {
        Self::with_provider(CPU_PROVIDER)
    }

    pub fn with_provider(provider: &str) -> Self {
        Self {
            default_provider: provider.to_ascii_lowercase(),
        }
    }

    fn resolve_provider<'a>(&'a self, preference: Option<&'a str>) -> Result<&'a str, EngineError> {
        let provider = preference.unwrap_or(&self.default_provider);
        if provider.eq_ignore_ascii_case(CPU_PROVIDER) {
            Ok(CPU_PROVIDER)
        } else {
            Err(EngineError::Provider(format!(
                "Execution provider '{provider}' is not enabled in this build; available providers: cpu"
            )))
        }
    }
}

pub struct OrtSession {
    session: Session,
    active_provider: String,
    input_shape: Option<[usize; 4]>,
    output_shape: Option<[usize; 4]>,
}

impl ModelSession for OrtSession {
    fn input_shape(&self) -> Option<[usize; 4]> {
        self.input_shape
    }

    fn output_shape(&self) -> Option<[usize; 4]> {
        self.output_shape
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl InferenceEngine for OrtEngine {
    fn id(&self) -> EngineId {
        EngineId("ort".to_string())
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            engine_id: self.id(),
            supported_providers: vec![CPU_PROVIDER.to_string()],
            supports_fp16: false,
            supports_dynamic_shapes: true,
        }
    }

    fn probe(&self) -> Result<EngineHealth, EngineError> {
        let provider = self.resolve_provider(None)?;
        let _ = ort::init().with_name("resvera").commit();
        Ok(EngineHealth {
            healthy: true,
            active_provider: provider.to_string(),
            diagnostic_message: Some(
                "ONNX Runtime initialized; model execution is validated when a session is loaded"
                    .to_string(),
            ),
        })
    }

    fn load(
        &self,
        model_bytes: &[u8],
        provider_preference: Option<&str>,
    ) -> Result<Box<dyn ModelSession>, EngineError> {
        let provider = self.resolve_provider(provider_preference)?;
        if model_bytes.is_empty() {
            return Err(EngineError::SessionLoad(
                "Model data is empty; a valid ONNX graph is required".to_string(),
            ));
        }

        let session = Session::builder()
            .map_err(|error| EngineError::SessionLoad(error.to_string()))?
            .commit_from_memory(model_bytes)
            .map_err(|error| EngineError::SessionLoad(error.to_string()))?;

        validate_model_contract(&session)?;
        let input_shape = static_rank_four_shape(session.inputs()[0].dtype());
        let output_shape = static_rank_four_shape(session.outputs()[0].dtype());

        Ok(Box::new(OrtSession {
            session,
            active_provider: provider.to_string(),
            input_shape,
            output_shape,
        }))
    }

    fn run(
        &self,
        session: &mut dyn ModelSession,
        input: TensorView<'_>,
        cancel: &CancellationToken,
    ) -> Result<OwnedTensor, EngineError> {
        cancel.check()?;
        validate_input(&input)?;

        let session = session
            .as_any_mut()
            .downcast_mut::<OrtSession>()
            .ok_or_else(|| {
                EngineError::SessionLoad(
                    "The supplied model session was not created by OrtEngine".to_string(),
                )
            })?;

        if session.active_provider != CPU_PROVIDER {
            return Err(EngineError::Provider(format!(
                "Session uses unsupported provider '{}'",
                session.active_provider
            )));
        }

        let input_tensor = TensorRef::from_array_view((input.shape, input.data))
            .map_err(|error| EngineError::InvalidTensor(error.to_string()))?;
        let outputs = session
            .session
            .run(ort::inputs![input_tensor])
            .map_err(|error| EngineError::Execution(error.to_string()))?;
        cancel.check()?;

        let first_output = outputs
            .values()
            .next()
            .ok_or_else(|| EngineError::Execution("Model produced no output tensor".to_string()))?;
        let (shape, data) = first_output
            .try_extract_tensor::<f32>()
            .map_err(|error| EngineError::Execution(error.to_string()))?;
        let shape = concrete_rank_four_shape(shape).ok_or_else(|| {
            EngineError::InvalidTensor(format!(
                "Expected a concrete rank-4 NCHW output, got {shape:?}"
            ))
        })?;

        OwnedTensor::new(shape, data.to_vec())
    }
}

fn validate_model_contract(session: &Session) -> Result<(), EngineError> {
    if session.inputs().len() != 1 || session.outputs().len() != 1 {
        return Err(EngineError::SessionLoad(format!(
            "Expected exactly one input and one output, got {} inputs and {} outputs",
            session.inputs().len(),
            session.outputs().len()
        )));
    }

    validate_outlet("input", session.inputs()[0].dtype())?;
    validate_outlet("output", session.outputs()[0].dtype())?;
    Ok(())
}

fn validate_outlet(label: &str, value_type: &ValueType) -> Result<(), EngineError> {
    match value_type {
        ValueType::Tensor { ty, shape, .. }
            if *ty == TensorElementType::Float32 && shape.len() == 4 =>
        {
            Ok(())
        }
        other => Err(EngineError::SessionLoad(format!(
            "Model {label} must be a rank-4 float32 NCHW tensor, got {other:?}"
        ))),
    }
}

fn static_rank_four_shape(value_type: &ValueType) -> Option<[usize; 4]> {
    match value_type {
        ValueType::Tensor { shape, .. } => concrete_rank_four_shape(shape),
        _ => None,
    }
}

fn concrete_rank_four_shape(shape: &[i64]) -> Option<[usize; 4]> {
    if shape.len() != 4 || shape.iter().any(|dimension| *dimension <= 0) {
        return None;
    }
    Some([
        usize::try_from(shape[0]).ok()?,
        usize::try_from(shape[1]).ok()?,
        usize::try_from(shape[2]).ok()?,
        usize::try_from(shape[3]).ok()?,
    ])
}

fn validate_input(input: &TensorView<'_>) -> Result<(), EngineError> {
    if input.shape.contains(&0) {
        return Err(EngineError::InvalidTensor(format!(
            "Tensor dimensions must be non-zero, got {:?}",
            input.shape
        )));
    }

    let expected_len = input.shape.iter().try_fold(1usize, |total, dimension| {
        total.checked_mul(*dimension).ok_or_else(|| {
            EngineError::InvalidTensor(format!(
                "Tensor shape {:?} exceeds addressable memory",
                input.shape
            ))
        })
    })?;
    if input.data.len() != expected_len {
        return Err(EngineError::InvalidTensor(format!(
            "Input data length mismatch: expected {expected_len} elements for shape {:?}, got {}",
            input.shape,
            input.data.len()
        )));
    }
    Ok(())
}
