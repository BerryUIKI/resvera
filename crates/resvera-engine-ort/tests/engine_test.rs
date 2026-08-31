use resvera_core::{CancellationToken, EngineError, InferenceEngine, OwnedTensor, TensorView};
use resvera_engine_ort::OrtEngine;

// A minimal ONNX opset-13 graph with one Identity node and a dynamic
// float32 NCHW contract: [1, 3, height, width] -> [1, 3, height, width].
const IDENTITY_NCHW_ONNX: &[u8] = &[
    8, 8, 18, 13, 114, 101, 115, 118, 101, 114, 97, 45, 116, 101, 115, 116, 115, 58, 126, 10, 25,
    10, 5, 105, 110, 112, 117, 116, 18, 6, 111, 117, 116, 112, 117, 116, 34, 8, 73, 100, 101, 110,
    116, 105, 116, 121, 18, 8, 105, 100, 101, 110, 116, 105, 116, 121, 90, 42, 10, 5, 105, 110,
    112, 117, 116, 18, 33, 10, 31, 8, 1, 18, 27, 10, 2, 8, 1, 10, 2, 8, 3, 10, 8, 18, 6, 104, 101,
    105, 103, 104, 116, 10, 7, 18, 5, 119, 105, 100, 116, 104, 98, 43, 10, 6, 111, 117, 116, 112,
    117, 116, 18, 33, 10, 31, 8, 1, 18, 27, 10, 2, 8, 1, 10, 2, 8, 3, 10, 8, 18, 6, 104, 101, 105,
    103, 104, 116, 10, 7, 18, 5, 119, 105, 100, 116, 104, 66, 2, 16, 13,
];

#[test]
fn probe_and_capabilities_are_truthful() {
    let engine = OrtEngine::new();
    let capabilities = engine.capabilities();

    assert_eq!(capabilities.engine_id.0, "ort");
    assert_eq!(capabilities.supported_providers, ["cpu"]);
    assert!(!capabilities.supports_fp16);

    let health = engine.probe().unwrap();
    assert!(health.healthy);
    assert_eq!(health.active_provider, "cpu");

    let unsupported = OrtEngine::with_provider("coreml").probe();
    assert!(matches!(unsupported, Err(EngineError::Provider(_))));
}

#[test]
fn executes_the_loaded_onnx_graph() {
    let engine = OrtEngine::with_provider("cpu");
    let mut session = engine.load(IDENTITY_NCHW_ONNX, Some("cpu")).unwrap();
    let input = OwnedTensor::new(
        [1, 3, 2, 2],
        vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.25],
    )
    .unwrap();
    let cancel = CancellationToken::new();

    let output = engine.run(&mut *session, input.view(), &cancel).unwrap();
    assert_eq!(output, input);
}

#[test]
fn rejects_invalid_models_tensors_and_cancelled_runs() {
    let engine = OrtEngine::with_provider("cpu");

    assert!(matches!(
        engine.load(b"not an ONNX graph", None),
        Err(EngineError::SessionLoad(_))
    ));

    let mut session = engine.load(IDENTITY_NCHW_ONNX, None).unwrap();
    let cancel = CancellationToken::new();
    let invalid = TensorView {
        shape: [1, 3, 2, 2],
        data: &[0.0; 3],
    };
    assert!(matches!(
        engine.run(&mut *session, invalid, &cancel),
        Err(EngineError::InvalidTensor(_))
    ));

    cancel.cancel();
    assert!(matches!(
        engine.run(
            &mut *session,
            TensorView {
                shape: [1, 3, 1, 1],
                data: &[0.0; 3],
            },
            &cancel,
        ),
        Err(EngineError::Cancelled)
    ));
}
