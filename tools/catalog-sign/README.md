# Catalog Signing Tool

Signs and verifies `crates/resvera-models/src/catalogs/production.json`.

The tool depends on `resvera-models` and calls the same `ModelCatalog::sign()` and
`verify()` methods the application uses. Do not reimplement the signing payload in
another language or with `serde_json::Value`: the payload is
`serde_json::to_vec()` over the structs, so key ordering is declaration order.
`Value` serializes maps in lexicographic order and produces signatures the runtime
rejects — this exact bug was hit while building this tool.

## Build

```bash
cargo build --release --manifest-path tools/catalog-sign/Cargo.toml
```

The tool is a standalone workspace, so it never joins the main workspace and
never runs in CI.

## Key custody

The production signing seed must never be committed. It is read from
`RESVERA_CATALOG_SIGNING_KEY` (lowercase hex) or `--key-file <path>`.

The seed previously committed to the test suite (`ResveraTrustRoot2026…`) was
rotated on 2026-11-01 because it was publicly derivable. The current trust root in
`crates/resvera-models/src/catalog.rs` corresponds to a key held out-of-tree.

Generate a replacement keypair when rotating:

```bash
tools/catalog-sign/target/release/resvera-catalog-sign keygen
```

Move `seed_hex` into the secret store immediately, then update
`RESVERA_PRODUCTION_TRUST_ROOT` in `crates/resvera-models/src/catalog.rs` to the
printed `public_key_hex`, and re-sign the catalog.

## Sign after editing the catalog

Any change to an entry (digest, URL, provider list, manifest template) invalidates
its signature. Always re-sign and re-verify:

```bash
tools/catalog-sign/target/release/resvera-catalog-sign sign \
  --catalog crates/resvera-models/src/catalogs/production.json \
  --key-file /secure/path/catalog-seed.hex

tools/catalog-sign/target/release/resvera-catalog-sign verify \
  --catalog crates/resvera-models/src/catalogs/production.json \
  --pubkey <RESVERA_PRODUCTION_TRUST_ROOT as hex>
```

`sign` also refreshes `manifest_sha256` from the manifest template bytes, because
the downloader hashes that string verbatim.

## Verification in CI

`cargo test -p resvera-models --test production_catalog_contract_test` verifies the
committed catalog against the pinned root without needing the seed. That test is
the gate; this tool is only for producing signatures.
