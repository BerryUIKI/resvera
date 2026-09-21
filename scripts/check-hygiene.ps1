$ErrorActionPreference = "Stop"

Write-Host "==> Running diff hygiene check..."
if (git rev-parse --verify HEAD~1 2>$null) {
    git diff --check HEAD~1
} else {
    git diff --check
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Running Rustfmt check across workspace..."
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Running Clippy with warnings denied..."
cargo clippy --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Running export & parity toolchain tests..."
python -m unittest discover -s tools/export
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> All hygiene checks passed successfully!"
