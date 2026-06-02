param(
    [string]$DebugDir = "target/neo-debug",
    [string]$Features = "ui-neo"
)

$ErrorActionPreference = "Stop"
$env:SKY_NEO_AGENT_DEBUG = "1"
$env:SKY_NEO_DEBUG_DIR = $DebugDir

Write-Host "[neo-debug] app=stress_lab debug-dir=$DebugDir"
Write-Host "[neo-debug] in another shell: cargo run --bin neo-debug --features ui-neo -- snapshot --max-rows 40"

cargo run --example ui_neo_stress_lab --features $Features
