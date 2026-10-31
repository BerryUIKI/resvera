//! Offline contract tests for the embedded production model catalog.
//!
//! These tests deliberately do **not** re-derive the catalog from the same
//! constants and assert that the result matches itself. They start from the
//! actually committed `src/catalogs/production.json` and check the invariants
//! that the install path relies on, so catalog drift fails here instead of at
//! `Install Model` time on a user's machine.
//!
//! What is covered offline:
//! - every install URL is HTTPS;
//! - every declared digest is well formed;
//! - catalog entry and manifest template agree on identity, family, variants,
//!   artifact path, artifact size, and artifact digest;
//! - `manifest_sha256` matches the template bytes that are actually staged;
//! - provenance digests are real, not placeholders;
//! - the distributable artifact is an ONNX graph, not upstream PyTorch weights;
//! - no model claims an execution provider the engine cannot run.
//!
//! Network reachability is intentionally **not** asserted here: CI must stay
//! hermetic and the offline inference boundary is a product requirement.

use resvera_models::catalog::{
    load_production_catalog, ModelCatalog, ModelCatalogEntry, RESVERA_PRODUCTION_TRUST_ROOT,
};
use resvera_models::manifest::ModelManifest;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Providers this build can actually execute. `OrtEngine::capabilities()`
/// reports CPU only, and `OrtEngine::resolve_provider()` rejects everything
/// else, so a catalog entry claiming more would surface a false claim in the
/// Model Center UI.
const ENGINE_EXECUTABLE_PROVIDERS: &[&str] = &["cpu"];

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Placeholder digests that indicate provenance was never filled in.
fn is_placeholder_digest(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.bytes().all(|byte| byte == b'0') || lowered.bytes().all(|byte| byte == b'f')
}

fn parse_manifest(entry: &ModelCatalogEntry) -> ModelManifest {
    let template = entry
        .manifest_template
        .as_deref()
        .unwrap_or_else(|| panic!("{}: catalog entry has no manifest_template", entry.id));
    serde_json::from_str(template).unwrap_or_else(|error| {
        panic!(
            "{}: manifest_template is not a valid manifest: {error}",
            entry.id
        )
    })
}

/// Reads the catalog exactly as a client would, including signature checks
/// against the pinned production trust root.
fn verified_catalog_entries() -> Vec<ModelCatalogEntry> {
    let catalog = load_production_catalog()
        .expect("committed production catalog must verify against the pinned trust root");
    assert!(
        !catalog.models.is_empty(),
        "production catalog must contain at least one model"
    );
    catalog.models
}

#[test]
fn catalog_entries_and_manifest_templates_agree() {
    for entry in verified_catalog_entries() {
        let manifest = parse_manifest(&entry);

        assert_eq!(
            manifest.id, entry.id,
            "{}: manifest id does not match catalog entry id",
            entry.id
        );
        assert_eq!(
            manifest.package_version, entry.version,
            "{}: manifest package_version does not match catalog entry version",
            entry.id
        );
        assert_eq!(
            manifest.family, entry.family,
            "{}: manifest family does not match catalog entry family",
            entry.id
        );

        // The downloader refuses to install unless the manifest declares exactly
        // one artifact at the canonical path with the catalog's size and digest.
        assert_eq!(
            manifest.artifacts.len(),
            1,
            "{}: manifest must declare exactly one artifact",
            entry.id
        );
        let artifact = &manifest.artifacts[0];
        assert_eq!(
            artifact.path, "artifacts/model.onnx",
            "{}: artifact must be at the canonical staging path",
            entry.id
        );
        assert_eq!(
            artifact.size_bytes, entry.size_bytes,
            "{}: manifest artifact size disagrees with catalog entry",
            entry.id
        );
        assert_eq!(
            artifact.sha256.to_ascii_lowercase(),
            entry.sha256.to_ascii_lowercase(),
            "{}: manifest artifact digest disagrees with catalog entry",
            entry.id
        );

        // Variants must be identical on both sides; a mismatch would install a
        // package whose selectable variants do not resolve.
        let entry_variants: Vec<(&str, u32)> = entry
            .variants
            .iter()
            .map(|variant| (variant.id.as_str(), variant.native_scale))
            .collect();
        let manifest_variants: Vec<(&str, u32)> = manifest
            .variants
            .iter()
            .map(|variant| (variant.id.as_str(), variant.native_scale))
            .collect();
        assert_eq!(
            entry_variants, manifest_variants,
            "{}: catalog variants do not match manifest variants",
            entry.id
        );

        // A variant that points at an artifact the manifest does not declare
        // makes the package unresolvable after install.
        for variant in &manifest.variants {
            assert!(
                manifest
                    .artifacts
                    .iter()
                    .any(|candidate| candidate.path == variant.artifact),
                "{}: variant '{}' references an undeclared artifact",
                entry.id,
                variant.id
            );
        }
    }
}

#[test]
fn manifest_template_digest_matches_staged_bytes() {
    // `StagedDownloader` hashes the template string bytes and compares against
    // `manifest_sha256`. Re-encoding the parsed manifest would produce different
    // bytes, so the digest must be computed over the committed template verbatim.
    for entry in verified_catalog_entries() {
        let template = entry
            .manifest_template
            .as_deref()
            .unwrap_or_else(|| panic!("{}: catalog entry has no manifest_template", entry.id));
        let computed = format!("{:x}", Sha256::digest(template.as_bytes()));
        assert_eq!(
            computed, entry.manifest_sha256,
            "{}: manifest_sha256 does not match the staged manifest bytes",
            entry.id
        );
        assert!(
            is_sha256_hex(&entry.manifest_sha256),
            "{}: manifest_sha256 is not a valid SHA-256 digest",
            entry.id
        );
    }
}

#[test]
fn catalog_digests_are_well_formed_and_not_placeholders() {
    for entry in verified_catalog_entries() {
        assert!(
            is_sha256_hex(&entry.sha256),
            "{}: artifact sha256 is not a 64-character hex digest",
            entry.id
        );

        let manifest = parse_manifest(&entry);
        assert!(
            !is_placeholder_digest(&manifest.provenance.source_weight_sha256),
            "{}: provenance.source_weight_sha256 is a placeholder, so the exported \
             artifact cannot be traced back to a specific upstream checkpoint",
            entry.id
        );
        assert!(
            is_sha256_hex(&manifest.provenance.source_weight_sha256),
            "{}: provenance.source_weight_sha256 is not a valid SHA-256 digest",
            entry.id
        );
        for (field, value) in [
            (
                "provenance.upstream_repository",
                &manifest.provenance.upstream_repository,
            ),
            (
                "provenance.upstream_revision",
                &manifest.provenance.upstream_revision,
            ),
            (
                "provenance.source_weight_name",
                &manifest.provenance.source_weight_name,
            ),
            (
                "provenance.export_recipe",
                &manifest.provenance.export_recipe,
            ),
        ] {
            assert!(
                !value.trim().is_empty(),
                "{}: {field} must be recorded so the artifact is traceable",
                entry.id
            );
        }
        assert!(
            manifest.license.redistribution_review == "approved",
            "{}: catalog entry is distributed but redistribution review is '{}'",
            entry.id,
            manifest.license.redistribution_review
        );
        assert!(
            !manifest.license.spdx.trim().is_empty(),
            "{}: license SPDX identifier must be declared",
            entry.id
        );
    }
}

#[test]
fn install_urls_are_https_and_point_at_onnx_artifacts() {
    // The catalog digest describes an ONNX graph. Distributing upstream PyTorch
    // weights under that digest can never verify, and would also require the
    // application to load a format it does not support.
    //
    // An empty `download_urls` list is legal and means "no published artifact
    // yet": the Model Center omits the install action rather than offering a
    // download that is guaranteed to fail. Every declared URL, however, must be
    // installable.
    for entry in verified_catalog_entries() {
        for url in &entry.download_urls {
            assert!(
                url.starts_with("https://"),
                "{}: install URL must use HTTPS, found '{url}'",
                entry.id
            );
            let lowered = url.to_ascii_lowercase();
            assert!(
                lowered.ends_with(".onnx"),
                "{}: install URL must serve an ONNX artifact, found '{url}'",
                entry.id
            );
            for forbidden in [".pth", ".pt", ".safetensors", ".ckpt"] {
                assert!(
                    !lowered.ends_with(forbidden),
                    "{}: install URL points at upstream weights ('{forbidden}'), \
                     which the digest and the engine contract cannot accept: '{url}'",
                    entry.id
                );
            }
        }
    }
}

#[test]
fn no_model_claims_providers_the_engine_cannot_run() {
    let executable: Vec<String> = ENGINE_EXECUTABLE_PROVIDERS
        .iter()
        .map(|provider| provider.to_ascii_lowercase())
        .collect();

    for entry in verified_catalog_entries() {
        let manifest = parse_manifest(&entry);

        // The UI renders `entry.validated_providers` verbatim.
        for provider in &entry.validated_providers {
            assert!(
                executable.iter().any(|known| known == provider),
                "{}: catalog entry advertises provider '{provider}', which the \
                 shipped engine cannot execute",
                entry.id
            );
        }
        // `resolve_request_model` also gates jobs on the manifest list.
        for provider in &manifest.compatibility.validated_providers {
            assert!(
                executable.iter().any(|known| known == provider),
                "{}: manifest advertises provider '{provider}', which the \
                 shipped engine cannot execute",
                entry.id
            );
        }

        assert!(
            !entry.validated_providers.is_empty(),
            "{}: catalog entry must advertise at least the CPU provider",
            entry.id
        );
    }
}

#[test]
fn catalog_ships_only_models_with_validated_packages() {
    // `docs/ROADMAP.md` §10 forbids listing a model in the desktop UI unless its
    // package passed deterministic export, signature validation, and fixture
    // parity. Real-CUGAN and Real-HAT-GAN have core adapters but no validated
    // package, so they must not appear in the production catalog at all — an
    // entry with a placeholder provenance digest would otherwise advertise an
    // installable model whose package cannot be traced or verified.
    let ids: Vec<String> = verified_catalog_entries()
        .into_iter()
        .map(|entry| entry.id)
        .collect();

    let expected = ["realesrgan-x4plus", "realesrgan-x4plus-anime"];
    assert_eq!(
        ids, expected,
        "production catalog model set changed; validate the new package before shipping it"
    );

    for entry in verified_catalog_entries() {
        let manifest = parse_manifest(&entry);
        assert!(
            manifest.license.redistribution_review == "approved",
            "{}: only redistribution-approved packages may ship",
            entry.id
        );
    }
}

#[test]
fn production_catalog_is_committed_bytes_not_a_test_fixture() {
    // Guard against a test regenerating `production.json` from a hard-coded
    // seed and then asserting on its own output. The committed catalog must be
    // valid JSON that verifies, and its recorded signature must be present.
    let raw = include_str!("../src/catalogs/production.json");
    let parsed: Value = serde_json::from_str(raw).expect("production.json must be valid JSON");
    let catalog: ModelCatalog =
        serde_json::from_str(raw).expect("production.json must deserialize as a catalog");

    assert!(
        !catalog.signature.trim().is_empty(),
        "production catalog must carry a signature"
    );

    let models = parsed
        .get("models")
        .and_then(Value::as_array)
        .expect("production.json must contain a models array");
    assert!(
        !models.is_empty(),
        "production catalog must declare at least one model"
    );
    for model in models {
        let id = model
            .get("id")
            .and_then(Value::as_str)
            .expect("each catalog model must have an id");
        assert!(
            !model
                .get("signature")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .is_empty(),
            "{id}: each catalog model must carry its own signature"
        );
    }

    // Signature verification against the pinned root is the real gate; this
    // assertion keeps the failure readable when the committed file is edited
    // without re-signing.
    catalog
        .verify(&RESVERA_PRODUCTION_TRUST_ROOT)
        .expect("committed production catalog must verify against the pinned trust root");
}
