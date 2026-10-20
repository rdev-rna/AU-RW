$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$out = Join-Path $root 'local/native-probe'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$vc = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
if (!(Test-Path -LiteralPath $vc)) { throw 'MSVC x64 BuildTools not installed at expected path.' }
$source = $PSScriptRoot
function Compile([string]$arguments) {
    $line = 'call "' + $vc + '" >nul && cl /nologo /std:c++17 /EHsc /O2 /W4 /DUNICODE /D_UNICODE ' + $arguments
    Push-Location $out
    try { & cmd.exe /d /s /c $line; if ($LASTEXITCODE) { throw "Compiler failed: $LASTEXITCODE" } }
    finally { Pop-Location }
}
Compile ('"' + $source + '\fixture.cpp" /Fe:acu_probe_fixture.exe /link /INCREMENTAL:NO')
$digest = (Get-FileHash -LiteralPath (Join-Path $out 'acu_probe_fixture.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath (Join-Path $out 'fixture_hash.h') -Value ('#define FIXTURE_SHA256 "' + $digest + '"') -Encoding Ascii
Compile ('"' + $source + '\launcher.cpp" /I"' + $out + '" /Fe:acu_probe_launcher.exe /link /INCREMENTAL:NO')
Compile ('/LD "' + $source + '\probe.cpp" /Fe:acu_probe.dll /link /INCREMENTAL:NO')
Copy-Item -LiteralPath (Join-Path $out 'acu_probe_launcher.exe') -Destination (Join-Path $out 'ACU-Probe.exe')
$paths = (& python (Join-Path $root 'tools/acu.py') paths | ConvertFrom-Json)
$gameExe = Join-Path $paths.game_dir 'ACU.exe'
Set-Content -LiteralPath (Join-Path $out 'launch.ini') -Value ("[launch]`r`nexe=" + $gameExe) -Encoding Unicode
Write-Output "Build output: $out"
