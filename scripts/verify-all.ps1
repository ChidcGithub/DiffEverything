param(
    [switch]$SkipBuild,
    [switch]$SkipSmoke
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path $PSScriptRoot -Parent
Push-Location $Root
try {
    function Assert-LastExit {
        param([string]$Step)
        if ($LASTEXITCODE -ne 0) {
            throw "$Step failed (exit $LASTEXITCODE)"
        }
        Write-Host "[PASS] $Step"
    }

    cargo fmt --all -- --check
    Assert-LastExit 'cargo fmt --check'

    cargo clippy --workspace --all-targets -- -D warnings
    Assert-LastExit 'cargo clippy'

    cargo test --workspace
    Assert-LastExit 'cargo test'

    if (-not $SkipBuild) {
        cargo build --release -p diff-cli
        Assert-LastExit 'cargo build --release'
    }

    if (-not $SkipSmoke) {
        if (-not (Test-Path (Join-Path $Root 'target\release\diff-everything.exe'))) {
            throw 'release binary not found; run without -SkipBuild first'
        }
        powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'smoke-test.ps1') -Exe (Join-Path $Root 'target\release\diff-everything.exe')
        Assert-LastExit 'release smoke'
    }

    Write-Host 'All verification steps passed.'
} finally {
    Pop-Location
}
