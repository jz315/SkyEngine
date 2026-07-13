param(
    [string]$DebugDir = "target/serein-debug",
    [string]$Features = "ui-serein"
)

$ErrorActionPreference = "Stop"
$env:SKY_SEREIN_AGENT_DEBUG = "1"
$env:SKY_SEREIN_DEBUG_DIR = $DebugDir

Write-Host "[serein-debug] app=stress_lab debug-dir=$DebugDir"
Write-Host "[serein-debug] in another shell: cargo run --bin serein-debug --features ui-serein -- snapshot --max-rows 40"

cargo run --example ui_serein_stress_lab --features $Features
