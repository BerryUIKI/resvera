# ADR-0003: Model & Runtime Package Format, Content Addressing, and Ed25519 Signing

## Status
Accepted

## Context
Resvera distributes pre-trained neural network models and runtime provider components to end-user machines. Because models and runtimes are executable code / computation graphs running locally with full user permissions:
1. Artifacts must be tamper-proof, content-addressed, and cryptographically signed.
2. Installations must be atomic to prevent partially written, corrupt, or unusable runtime states.
3. Upgrades and rollbacks must be seamless, allowing instant recovery to previously installed working packages without re-downloading.
4. Model conversion recipes and upstream provenance must be strictly recorded for reproducibility and licensing compliance.

## Decision
1. **Package Directory Structure:**
   Each model is packaged into an immutable directory structure:
   ```text
   <app_data_dir>/models/<model-id>/<package-version>/
   ├── manifest.json
   ├── checksums.json
   ├── LICENSE.txt
   ├── NOTICE.md
   └── artifacts/
       ├── model.onnx
       └── ... (additional variants for Real-CUGAN, etc.)
   ```
2. **Cryptographic Signing and Integrity Verification:**
   - Model and runtime catalogs (`models.signed.json`, `runtimes.signed.json`) are signed using **Ed25519** public-key cryptography.
   - Resvera embeds trusted root public keys in source control with support for key rotation.
   - `checksums.json` contains SHA-256 hashes of every file in the package.
   - Every artifact is checked against its declared SHA-256 hash prior to installation.
3. **Atomic Installation and Pointer Switching:**
   - Downloads and temporary extractions occur inside `<app_data_dir>/transactions/<tx-id>/`.
   - Structural ONNX validation (verifying node graphs and tensor inputs/outputs) is executed in the transaction staging area.
   - Upon verification success, the directory is moved atomically via filesystem rename into `models/<model-id>/<package-version>/`.
   - The active package version is referenced via an atomic pointer `models/<model-id>/current.json`.
4. **Rollback Mechanism:**
   - Previous working package versions are retained according to the cache retention policy.
   - In case of failure or user request, `activate_model_version` updates `current.json` back to the earlier package version instantaneously without network access.

## Consequences
### Positive
- Strict protection against man-in-the-middle tampering, corruption, and partial downloads.
- Guaranteed offline resilience: an existing working version is never destroyed until the replacement is verified.
- Clear traceability of model licenses, authors, and export recipes.

### Negative / Trade-offs
- Retaining previous versions consumes disk storage until pruned by retention cleanup.
