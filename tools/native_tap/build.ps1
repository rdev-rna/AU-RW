# Builds acu_tap.dll (MinHook from local/native-probe/third_party/minhook, BSD-2).
param([string]$Name = 'acu_tap3')
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$out = Join-Path $root 'local/native-probe'
$mh = Join-Path $out 'third_party/minhook'
$vc = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
$src = $PSScriptRoot
$line = 'call "' + $vc + '" >nul && ml64 /nologo /c /Fo tap_thunk.obj "' + $src + '\thunk.asm" && ' +
  'cl /nologo /O2 /W3 /EHsc /LD /std:c++17 /I"' + $mh + '\include" "' + $src + '\tap.cpp" tap_thunk.obj "' + $mh + '\src\hook.c" "' + $mh + '\src\buffer.c" "' + $mh + '\src\trampoline.c" "' + $mh + '\src\hde\hde64.c" /Fe:' + $Name + '.dll /link /INCREMENTAL:NO'
Push-Location $out
try { & cmd.exe /d /s /c $line; if ($LASTEXITCODE) { throw "build failed: $LASTEXITCODE" } } finally { Pop-Location }
(Get-FileHash (Join-Path $out ($Name + '.dll')) -Algorithm SHA256).Hash.ToLowerInvariant()
$line2 = 'call "' + $vc + '" >nul && cl /nologo /O1 /W3 /EHsc /std:c++17 "' + $src + '\selftest.cpp" /Fe:acu_tap_selftest.exe /link /INCREMENTAL:NO'
Push-Location $out
try { & cmd.exe /d /s /c $line2; if ($LASTEXITCODE) { throw "selftest build failed: $LASTEXITCODE" } } finally { Pop-Location }
$line3 = 'call "' + $vc + '" >nul && cl /nologo /O1 /W3 /EHsc /std:c++17 "' + $src + '\launch.cpp" /Fe:acu_tap_launch.exe /link /INCREMENTAL:NO'
Push-Location $out
try { & cmd.exe /d /s /c $line3; if ($LASTEXITCODE) { throw "launcher build failed: $LASTEXITCODE" } } finally { Pop-Location }
