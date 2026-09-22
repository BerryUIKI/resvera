# Resvera Toolchain Specification & Update Procedures

This document defines the exact, reproducible toolchain versions required to build, test, export models, and package Resvera. All local developer environments and continuous integration (CI) workflows MUST adhere to these exact versions.

---

## 1. Pinned Toolchain Matrix

| Tool / Runtime | Pinned Version | Canonical Configuration Source | CI Enforcement |
|---|---|---|---|
| **Rust Toolchain** | `1.97.1` | `rust-toolchain.toml` | `dtolnay/rust-toolchain` via `rust-toolchain.toml` |
| **Node.js** | `22.13.0` | `.node-version`, `package.json#engines` | `actions/setup-node@v4` with `node-version-file: .node-version` |
| **pnpm** | `11.18.0` | `package.json#packageManager` | `pnpm/action-setup@v4` with `version: 11.18.0` |
| **Python** | `3.12.7` | `.python-version` | `actions/setup-python@v5` with `python-version-file: .python-version` |
| **Model Export Toolchain** | Exact pins | `tools/export/requirements.txt`, `requirements-lock.txt` | `pip install -r tools/export/requirements.txt` |

---

## 2. Pinned Python Export Toolchain

The ONNX model export and golden-image parity validation suite (`tools/export` and `tools/parity`) rely on exact, reproducible ML package releases:

- `torch==2.4.1`
- `onnx==1.16.2`
- `onnxruntime==1.19.2`
- `numpy==1.26.4`
- `pillow==10.4.0`
- `scipy==1.14.1`

All indirect dependencies are locked in `tools/export/requirements-lock.txt`.

---

## 3. Toolchain Update Procedures

When upgrading any toolchain component, follow these disciplined procedures:

### 3.1 Rust Toolchain Upgrades
1. Update `channel` in `rust-toolchain.toml`:
   ```toml
   [toolchain]
   channel = "<new-version>"
   components = ["rustfmt", "clippy"]
   profile = "minimal"
   ```
2. Update local rustup:
   ```bash
   rustup update <new-version>
   ```
3. Run formatting, lints, and workspace test suite:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test --workspace
   ```
4. If dependencies changed, verify and commit updated `Cargo.lock`.

### 3.2 Node.js & pnpm Upgrades
1. Update `.node-version`:
   ```text
   <new-node-version>
   ```
2. Update `package.json`:
   ```json
   "packageManager": "pnpm@<new-pnpm-version>",
   "engines": {
     "node": ">=<new-node-version>",
     "pnpm": ">=<new-pnpm-version>"
   }
   ```
3. Re-resolve dependencies and update lockfile:
   ```bash
   pnpm install --frozen-lockfile=false
   pnpm run check
   pnpm run build
   ```
4. Commit updated `package.json` and `pnpm-lock.yaml`.

### 3.3 Python & Export Toolchain Upgrades
1. Update `.python-version`:
   ```text
   <new-python-version>
   ```
2. Update `tools/export/requirements.txt` with exact `==` pins.
3. In a fresh virtual environment:
   ```bash
   pip install -r tools/export/requirements.txt
   pip freeze > tools/export/requirements-lock.txt
   ```
4. Run the export integrity and numerical parity test suite:
   ```bash
   python -m unittest discover -s tools/export
   ```
5. Verify determinism: test export on representative checkpoint and verify byte-for-byte identical output SHA-256 digests.
6. Commit `.python-version`, `requirements.txt`, and `requirements-lock.txt`.

---

## 4. Reproducibility Verification

Automated CI and test suites enforce that:
- No toolchain file specifies unpinned ranges (`stable`, `latest`, `>=`, `^`, `~`).
- Model export is deterministic: running export twice with the same inputs produces identical SHA-256 hashes.
- Build artifacts are reproducible across clean builds on the same platform.
