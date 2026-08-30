use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Out of memory: {0}")]
    OutOfMemory(String),
    #[error("Session load error: {0}")]
    SessionLoad(String),
    #[error("Inference execution error: {0}")]
    Execution(String),
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Cancelled")]
    Cancelled,
    #[error("Unsupported shape or layout: {0}")]
    InvalidTensor(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineId(pub String);

#[derive(Debug, Clone)]
pub struct EngineCapabilities {
    pub engine_id: EngineId,
    pub supported_providers: Vec<String>,
    pub supports_fp16: bool,
    pub supports_dynamic_shapes: bool,
}

#[derive(Debug, Clone)]
pub struct EngineHealth {
    pub healthy: bool,
    pub active_provider: String,
    pub diagnostic_message: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> Result<(), EngineError> {
        if self.is_cancelled() {
            Err(EngineError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub struct TensorView<'a> {
    pub shape: [usize; 4], // [Batch, Channels, Height, Width]
    pub data: &'a [f32],
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnedTensor {
    pub shape: [usize; 4], // [Batch, Channels, Height, Width]
    pub data: Vec<f32>,
}

impl OwnedTensor {
    pub fn new(shape: [usize; 4], data: Vec<f32>) -> Result<Self, EngineError> {
        let expected_len = shape.iter().try_fold(1usize, |total, dimension| {
            total.checked_mul(*dimension).ok_or_else(|| {
                EngineError::InvalidTensor(format!(
                    "Tensor shape {:?} exceeds addressable memory",
                    shape
                ))
            })
        })?;
        if data.len() != expected_len {
            return Err(EngineError::InvalidTensor(format!(
                "Data length {} does not match shape {:?} (expected {})",
                data.len(),
                shape,
                expected_len
            )));
        }
        Ok(Self { shape, data })
    }

    pub fn view(&self) -> TensorView<'_> {
        TensorView {
            shape: self.shape,
            data: &self.data,
        }
    }
}

pub trait ModelSession: Any + Send + Sync {
    fn input_shape(&self) -> Option<[usize; 4]>;
    fn output_shape(&self) -> Option<[usize; 4]>;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub trait InferenceEngine: Send + Sync {
    fn id(&self) -> EngineId;
    fn capabilities(&self) -> EngineCapabilities;
    fn probe(&self) -> Result<EngineHealth, EngineError>;
    fn load(
        &self,
        model_bytes: &[u8],
        provider_preference: Option<&str>,
    ) -> Result<Box<dyn ModelSession>, EngineError>;
    fn run(
        &self,
        session: &mut dyn ModelSession,
        input: TensorView<'_>,
        cancel: &CancellationToken,
    ) -> Result<OwnedTensor, EngineError>;
}
