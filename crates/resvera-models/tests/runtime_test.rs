use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use resvera_models::{
    sign_payload, RuntimeArtifact, RuntimeCatalog, RuntimeComponentManifest, RuntimeInstaller,
    SignedRuntimeCatalog,
};
use sha2::{Digest, Sha256};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_signed_runtime_catalog_and_atomic_installation() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let secret_bytes = signing_key.to_bytes();
    let pub_key_bytes = signing_key.verifying_key().to_bytes();
    let pub_key_hex = hex::encode(pub_key_bytes);

    let dummy_data = b"MOCK_DIRECTML_NATIVE_BINARY_PAYLOAD";
    let expected_hash = format!("{:x}", Sha256::digest(dummy_data));
    let manifest = RuntimeComponentManifest {
        id: "directml".into(),
        version: "1.15.2".into(),
        display_name: "DirectML Acceleration Runtime".into(),
        target_platform: "windows-x86_64".into(),
        min_engine_version: "1.29.0".into(),
        artifacts: vec![RuntimeArtifact {
            name: "DirectML.dll".into(),
            sha256: expected_hash,
            size_bytes: dummy_data.len() as u64,
        }],
    };

    let catalog = RuntimeCatalog {
        schema_version: 1,
        updated_at: "2026-08-29T12:00:00Z".into(),
        components: vec![manifest.clone()],
    };

    let payload = serde_json::to_string(&catalog).unwrap();
    let signature = sign_payload(payload.as_bytes(), &secret_bytes);

    let signed_catalog = SignedRuntimeCatalog { payload, signature };

    // 1. Verify and parse signed runtime catalog
    let verified = signed_catalog.verify_and_parse(&pub_key_hex).unwrap();
    assert_eq!(verified.components.len(), 1);
    assert_eq!(verified.components[0].id, "directml");

    // 2. Prepare mock staged artifact
    let temp = tempdir().unwrap();
    let runtime_dir = temp.path().join("runtimes");
    let staging_dir = temp.path().join("staging");
    fs::create_dir_all(&staging_dir).unwrap();

    let dummy_dll = staging_dir.join("DirectML.dll");
    fs::write(&dummy_dll, dummy_data).unwrap();

    let installer = RuntimeInstaller::new(&runtime_dir);

    // 3. Successful atomic install
    let staged_files = vec![dummy_dll.clone()];
    let installed_path = installer
        .install_component(&manifest, &staged_files)
        .unwrap();

    assert!(installed_path.join("DirectML.dll").exists());

    // 4. Failed install due to hash mismatch (must not corrupt existing)
    let bad_staging_dir = temp.path().join("bad-staging");
    fs::create_dir_all(&bad_staging_dir).unwrap();
    let bad_dll = bad_staging_dir.join("DirectML.dll");
    fs::write(&bad_dll, b"tampered runtime").unwrap();
    let bad_staged = vec![bad_dll];
    let res = installer.install_component(&manifest, &bad_staged);
    assert!(res.is_err());
    assert_eq!(
        fs::read(installed_path.join("DirectML.dll")).unwrap(),
        dummy_data
    );

    // 5. Active-version replacement works even on platforms that cannot rename over a file.
    installer.rollback_component("directml", "1.15.2").unwrap();
    installer.rollback_component("directml", "1.15.2").unwrap();
    assert_eq!(
        fs::read_to_string(runtime_dir.join("directml/active_version.txt")).unwrap(),
        "1.15.2"
    );
}
