param([string]$GameDir = $env:ACU_GAME_DIR, [int]$ViewerId = 0)

# Launch only this project's viewer. Inject inputs only while its own window is
# foreground, and capture only that window. Proprietary pixels remain local.
$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSScriptRoot
if (-not $GameDir) { $GameDir = & python (Join-Path $PSScriptRoot 'acu.py') paths --key game_dir }
$exe = Join-Path $projectDir 'acu-rs/target/debug/acu_bevy.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'Build acu_bevy first.' }
$output = Join-Path $projectDir ('local/captures/acu-arno-window-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $output | Out-Null
if (-not ('AuRwWindowReview' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AuRwWindowReview {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr w, out Rect r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr w);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr w, out uint p);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr w, int c);
    [DllImport("user32.dll")] public static extern void keybd_event(byte k, byte s, uint f, UIntPtr e);
    [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint k, uint mode);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool attach);
}
'@
}
Add-Type -AssemblyName System.Drawing
[AuRwWindowReview]::SetProcessDPIAware() | Out-Null
$previousBackend = $env:WGPU_BACKEND
$env:WGPU_BACKEND = 'dx12'
$arguments = '--game-dir "' + $GameDir + '" --report "' + (Join-Path $output 'scene.json') + '"'
try {
    if ($ViewerId) {
        $viewer = Get-Process -Id $ViewerId
        if ($viewer.Path -ne $exe) { throw 'Existing process is not this project viewer.' }
        $arguments = (Get-CimInstance Win32_Process -Filter "ProcessId=$ViewerId").CommandLine
    } else {
        $viewer = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $projectDir -PassThru -WindowStyle Normal -RedirectStandardOutput (Join-Path $output 'stdout.log') -RedirectStandardError (Join-Path $output 'stderr.log')
    }
} finally { $env:WGPU_BACKEND = $previousBackend }
$deadline = (Get-Date).AddSeconds(55)
do {
    Start-Sleep -Milliseconds 250
    $viewer.Refresh()
    if ($viewer.HasExited) { throw "Viewer exited: $($viewer.ExitCode). See $output" }
    $ready = if ($ViewerId) { $true } else { Select-String -LiteralPath (Join-Path $output 'stdout.log') -Pattern 'CPU skin mean over 60 frames' -Quiet }
} until (($ready -and $viewer.MainWindowHandle -ne 0) -or (Get-Date) -gt $deadline)
if (-not $ready) { throw "Viewer did not finish warmup: $output" }
$window = $viewer.MainWindowHandle
[uint32]$owner = 0
[AuRwWindowReview]::GetWindowThreadProcessId($window, [ref]$owner) | Out-Null
if ($owner -ne $viewer.Id) { throw 'Window ownership mismatch.' }
[AuRwWindowReview]::ShowWindow($window, 9) | Out-Null
[uint32]$foregroundOwner = 0
$foregroundThread = [AuRwWindowReview]::GetWindowThreadProcessId([AuRwWindowReview]::GetForegroundWindow(), [ref]$foregroundOwner)
$ownThread = [AuRwWindowReview]::GetCurrentThreadId()
$attached = $foregroundThread -ne 0 -and $foregroundThread -ne $ownThread -and [AuRwWindowReview]::AttachThreadInput($ownThread, $foregroundThread, $true)
try { [AuRwWindowReview]::SetForegroundWindow($window) | Out-Null }
finally { if ($attached) { [AuRwWindowReview]::AttachThreadInput($ownThread, $foregroundThread, $false) | Out-Null } }
Start-Sleep -Milliseconds 500
$events = [Collections.Generic.List[object]]::new()
$captures = [Collections.Generic.List[object]]::new()
function Key([byte]$Code, [bool]$Down) {
    if ($Down -and [AuRwWindowReview]::GetForegroundWindow() -ne $window) { throw 'Own viewer must remain foreground for input review.' }
    [uint32]$flags = if ($Down) { 0 } else { 2 }
    # Navigation keys need the extended bit; otherwise winit sees numpad keys.
    if ($Code -ge 0x25 -and $Code -le 0x28) { $flags = $flags -bor 1 }
    [AuRwWindowReview]::keybd_event($Code, [byte][AuRwWindowReview]::MapVirtualKey($Code, 0), $flags, [UIntPtr]::Zero)
    $events.Add(@{ time_utc = [DateTime]::UtcNow.ToString('o'); virtual_key = $Code; down = $Down })
}
function Capture([string]$Name) {
    $rect = [AuRwWindowReview+Rect]::new()
    if (-not [AuRwWindowReview]::GetWindowRect($window, [ref]$rect)) { throw 'GetWindowRect failed.' }
    $bitmap = [Drawing.Bitmap]::new($rect.Right - $rect.Left, $rect.Bottom - $rect.Top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $path = Join-Path $output ($Name + '.png')
        $bitmap.Save($path, [Drawing.Imaging.ImageFormat]::Png)
        $captures.Add(@{ name = $Name; path = $path; sha256 = (Get-FileHash -LiteralPath $path).Hash.ToLower(); width = $bitmap.Width; height = $bitmap.Height })
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}
try {
    Capture 'idle'
    Key 0x57 $true; Start-Sleep -Milliseconds 1100; Capture 'walk'
    Key 0x10 $true; Start-Sleep -Milliseconds 900; Capture 'run'
    Key 0x20 $true; Start-Sleep -Milliseconds 60; Key 0x20 $false
    Start-Sleep -Milliseconds 220; Capture 'jump'
    Key 0x57 $false; Key 0x10 $false
    Start-Sleep -Milliseconds 600; Capture 'stop'
    Key 0x52 $true; Start-Sleep -Milliseconds 100; Key 0x52 $false
    Start-Sleep -Milliseconds 300; Capture 'reset'
    Key 0x27 $true; Start-Sleep -Milliseconds 600; Key 0x27 $false
    Start-Sleep -Milliseconds 100
    Capture 'orbit'
    Key 0x26 $true; Start-Sleep -Milliseconds 350; Key 0x26 $false
    Start-Sleep -Milliseconds 100
    Capture 'pitch'
    Key 0x51 $true; Start-Sleep -Milliseconds 300; Key 0x51 $false
    Capture 'zoom'
    Key 0x71 $true; Start-Sleep -Milliseconds 60; Key 0x71 $false
    Start-Sleep -Milliseconds 250; Capture 'bind'
    Key 0x71 $true; Start-Sleep -Milliseconds 60; Key 0x71 $false
} finally {
    foreach ($code in @(0x57,0x10,0x20,0x52,0x27,0x26,0x51,0x71)) { Key $code $false }
    @{ subject = 'rewrite'; backend = 'dx12'; executable = $exe; executable_sha256 = (Get-FileHash -LiteralPath $exe).Hash.ToLower(); arguments = $arguments; reused_process = [bool]$ViewerId; process_id = $viewer.Id; events = $events; captures = $captures; limits = 'Wall-clock OS keyboard review; fixed deterministic scenarios are tested separately. No native game comparison or physical controller validation.' } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $output 'review.json') -Encoding UTF8
}
Write-Output "Window review: $output (viewer PID $($viewer.Id), left running)"
