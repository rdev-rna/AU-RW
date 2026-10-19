param(
    [string]$GameDir = $env:ACU_GAME_DIR,
    [string]$Capture,
    [float]$Orbit = 35,
    [ValidateSet('dx12', 'vulkan')][string]$Backend,
    [switch]$NoTextures,
    [switch]$Static,
    [ValidateSet('idle','bind','deform','walk','run','turn','jump','contact','profile-up','profile-down')][string]$Scenario = 'idle',
    [ValidateRange(0,36000)][int]$Ticks = 0
)

$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSScriptRoot
if (-not $GameDir) {
    $GameDir = & python (Join-Path $PSScriptRoot 'acu.py') paths --key game_dir
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve game path; run tools/acu.py configure.' }
}
if (-not (Test-Path -LiteralPath (Join-Path $GameDir 'ACU.exe') -PathType Leaf)) {
    throw 'ACU.exe is unavailable. Configure local/project.json, set ACU_GAME_DIR or pass -GameDir.'
}
$reportDir = Join-Path $projectDir 'local/captures/acu-arno-rigged-20261019'
New-Item -ItemType Directory -Force -Path $reportDir | Out-Null
$sceneArgs = @('--game-dir', $GameDir, '--orbit-deg', "$Orbit", '--scenario', $Scenario, '--ticks', "$Ticks", '--report', (Join-Path $reportDir "scene-$Scenario-$Ticks-$Orbit.json"))
if ($Capture) { $sceneArgs += @('--capture', [System.IO.Path]::GetFullPath($Capture)) }
if ($NoTextures) { $sceneArgs += '--no-textures' }
if ($Static) { $sceneArgs += '--static' }
Push-Location (Join-Path $projectDir 'acu-rs')
$previousBackend = $env:WGPU_BACKEND
try {
    $env:WGPU_BACKEND = if ($Backend) { $Backend } elseif ($Capture) { 'vulkan' } else { 'dx12' }
    # Left stick move; RT/R2 hold high profile; +South (PS X/Xbox A) up;
    # +East (PS Circle/Xbox B) down; right stick camera yaw and pitch.
    # WASD move; Shift high profile; Shift+Space up; Shift+C down; R reset.
    # F2 bind pose; F3 freeze pose; Escape exits. Authored test locomotion.
    # Windows PowerShell treats redirected native stderr as ErrorRecords;
    # cargo writes normal progress there. Judge native failure by exit status.
    $ErrorActionPreference = 'Continue'
    & cargo run --locked -p acu_bevy -- @sceneArgs
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -ne 0) { throw "Arno scene failed with exit code $LASTEXITCODE" }
} finally {
    $env:WGPU_BACKEND = $previousBackend
    Pop-Location
}
