[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command '$Name' was not found in PATH."
    }
}

function Invoke-Python([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments) {
    if (Get-Command py -ErrorAction SilentlyContinue) {
        & py -3 @Arguments
    }
    elseif (Get-Command python -ErrorAction SilentlyContinue) {
        & python @Arguments
    }
    else {
        throw "Python 3 was not found. Install Python 3 and ensure either 'py' or 'python' is available."
    }
    if ($LASTEXITCODE -ne 0) { throw "Python command failed with exit code $LASTEXITCODE." }
}

Write-Host "== GVYA Engine v1 / Windows build =="
Write-Host "Root: $Root"

Require-Command node
Require-Command npm
Require-Command rustup
Require-Command cargo
Require-Command rustc

$NodeVersion = (& node --version).Trim()
if ($LASTEXITCODE -ne 0) { throw "node --version failed." }
if ($NodeVersion -notmatch '^v(?<major>\d+)\.') { throw "Could not parse Node version '$NodeVersion'." }
if ([int]$Matches.major -lt 24) { throw "GVYA requires Node >= 24; found $NodeVersion." }
Write-Host "Node: $NodeVersion"

Write-Host "`n== Install/pin Rust 1.85.0 and wasm32 target =="
& rustup toolchain install 1.85.0 --profile minimal
if ($LASTEXITCODE -ne 0) { throw "rustup toolchain install failed." }
& rustup component add rustfmt clippy --toolchain 1.85.0
if ($LASTEXITCODE -ne 0) { throw "rustup component add failed." }
& rustup target add wasm32-unknown-unknown --toolchain 1.85.0
if ($LASTEXITCODE -ne 0) { throw "rustup target add wasm32-unknown-unknown failed." }

$env:RUSTUP_TOOLCHAIN = "1.85.0"
$RustVersion = (& rustc --version).Trim()
if ($LASTEXITCODE -ne 0 -or -not $RustVersion.StartsWith("rustc 1.85.0 ")) {
    throw "Expected rustc 1.85.0, found '$RustVersion'."
}
Write-Host "Rust: $RustVersion"

Write-Host "`n== Remove stale Rust build outputs =="
& cargo clean
if ($LASTEXITCODE -ne 0) { throw "cargo clean failed." }

Write-Host "`n== Install Studio dev dependency if needed =="
if (-not (Test-Path "node_modules\typescript\package.json")) {
    & npm install --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw "npm install failed." }
}

Write-Host "`n== Rust formatting =="
& cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw "cargo fmt --all -- --check failed." }

Write-Host "`n== Rust workspace tests (warnings are denied by workspace lint policy) =="
& cargo test --workspace
if ($LASTEXITCODE -ne 0) { throw "cargo test --workspace failed." }

Write-Host "`n== Build canonical single Engine WASM v1 =="
Invoke-Python "tools/build_engine_assets.py"

Write-Host "`n== Verify Engine v1 identity, integrity, and ABI exports =="
Invoke-Python "tools/verify_engine_assets.py"

Write-Host "`n== Source / Studio / SDK / AI validation =="
& npm run test:source
if ($LASTEXITCODE -ne 0) { throw "npm run test:source failed." }

Write-Host "`n== Studio Engine bridge contract =="
& node validation/studio-engine-contract.mjs
if ($LASTEXITCODE -ne 0) { throw "Studio Engine bridge contract failed." }

Write-Host "`n== Headless Engine v1 acceptance =="
& node validation/engine-v1-acceptance.mjs
if ($LASTEXITCODE -ne 0) { throw "Engine v1 acceptance failed." }

Write-Host "`n== Clean generated build outputs before source manifest closure =="
& npm run clean
if ($LASTEXITCODE -ne 0) { throw "npm run clean failed." }

Write-Host "`n== Refresh canonical source manifests after adding Engine assets =="
Invoke-Python "tools/validate-source.py" "--write-manifests"
Invoke-Python "tools/validate-source.py"

$EngineDir = Join-Path $Root "apps\studio\public\engine\v1"
$Engine = Join-Path $EngineDir "gvya-ffi.wasm"
$Manifest = Join-Path $EngineDir "manifest.json"

Write-Host "`n== Final Engine v1 files =="
Get-Item $Engine, $Manifest | Select-Object Name, Length, LastWriteTime
Write-Host ""
Get-FileHash -Algorithm SHA256 $Engine | Select-Object Path, Hash

Write-Host "`nSUCCESS: GVYA Engine v1 single WASM built and accepted."
Write-Host "Upload these two files back to ChatGPT:"
Write-Host "  $Engine"
Write-Host "  $Manifest"
