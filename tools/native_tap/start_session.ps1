# Launch ACU via Ubisoft Connect, inject the tap DLL (-DllStem, default acu_tap6), drive the title/Continue menus with the virtual pad.
# Output: pid on the last line. Needs the DLLs built by build.ps1 and the ViGEm setup used by controller_autoplay.py.
param([int]$Attempts = 8, [string]$DllStem = 'acu_tap6')
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$np = Join-Path $root 'local/native-probe'
$sha = '748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40'
Remove-Item (Join-Path $np 'tap-forensics-*.log') -ErrorAction SilentlyContinue
& (Join-Path $np 'acu_tap_launch.exe') --attempts $Attempts --settle-seconds 25 --dll-stem $DllStem | Tee-Object -Variable launchLog | Out-Host
if (-not ($launchLog -match 'TAP READY')) { throw 'launcher did not get a sustained injected session' }
$gamePid = (Get-Process ACU).Id
& (Join-Path $np 'acu_probe_launcher.exe') --attach $gamePid --dll (Join-Path $np 'acu_probe.dll') | Out-Null
Add-Type @"
using System; using System.Runtime.InteropServices;
public class FG { [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int c); [DllImport("user32.dll")] public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e); [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); }
"@
$p = Get-Process ACU
$age = ((Get-Date) - $p.StartTime).TotalSeconds
if ($age -lt 125) { Start-Sleep -Seconds ([int](125 - $age)) }   # the title ignores input until loading finishes (~2 min)
for ($i = 0; $i -lt 6; $i++) { [FG]::keybd_event(0x12,0,0,[UIntPtr]::Zero); [FG]::ShowWindow($p.MainWindowHandle,9) | Out-Null; [FG]::SetForegroundWindow($p.MainWindowHandle) | Out-Null; [FG]::keybd_event(0x12,0,2,[UIntPtr]::Zero); Start-Sleep -Milliseconds 700; if ([FG]::GetForegroundWindow() -eq $p.MainWindowHandle) { break } }
[FG]::keybd_event(0x12,0,0,[UIntPtr]::Zero); [FG]::ShowWindow($p.MainWindowHandle,9) | Out-Null; [FG]::SetForegroundWindow($p.MainWindowHandle) | Out-Null; [FG]::keybd_event(0x12,0,2,[UIntPtr]::Zero)
Start-Sleep 1
$run = 'local/native-probe/run-menu-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
Push-Location $root
try { python tools/native_probe/controller_autoplay.py $gamePid local/native-probe/menu_then_world-20261020.json $run --expected-sha256 $sha --snapshot none --screenshot-step world_neutral | Out-Host } finally { Pop-Location }
if (-not (Get-Process -Id $gamePid -ErrorAction SilentlyContinue)) { throw 'game exited during menu entry' }
Write-Output $gamePid
