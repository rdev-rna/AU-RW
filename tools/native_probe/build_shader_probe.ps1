# Builds local/native-probe/acu_shader_probe.dll (shader_probe.cpp; MinHook from
# local/native-probe/third_party/minhook, BSD-2, as tools/native_tap/build.ps1 uses it).
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$out = Join-Path $root 'local/native-probe'
$mh = Join-Path $out 'third_party/minhook'
$vc = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
$src = $PSScriptRoot
$line = 'call "' + $vc + '" >nul && cl /nologo /O2 /W3 /EHsc /LD /std:c++17 /I"' + $mh + '\include" "' + $src +
  '\shader_probe.cpp" "' + $mh + '\src\hook.c" "' + $mh + '\src\buffer.c" "' + $mh + '\src\trampoline.c" "' + $mh +
  '\src\hde\hde64.c" /Fo:shader_probe_obj\ /Fe:acu_shader_probe.dll /link /INCREMENTAL:NO'
New-Item -ItemType Directory -Force -Path (Join-Path $out 'shader_probe_obj') | Out-Null
Push-Location $out
try { & cmd.exe /d /s /c $line; if ($LASTEXITCODE) { throw "build failed: $LASTEXITCODE" } } finally { Pop-Location }
(Get-FileHash (Join-Path $out 'acu_shader_probe.dll') -Algorithm SHA256).Hash.ToLowerInvariant()
# acu_hbao_probe.dll (hbao_probe.cpp): HBAO+ RenderAO parameters, MinHook.
$line3 = 'call "' + $vc + '" >nul && cl /nologo /O2 /W3 /EHa /LD /std:c++17 /I"' + $mh + '\include" "' + $src +
  '\hbao_probe.cpp" "' + $mh + '\src\hook.c" "' + $mh + '\src\buffer.c" "' + $mh + '\src\trampoline.c" "' + $mh +
  '\src\hde\hde64.c" /Fo:shader_probe_obj\ /Fe:acu_hbao_probe.dll /link /INCREMENTAL:NO'
Push-Location $out
try { & cmd.exe /d /s /c $line3; if ($LASTEXITCODE) { throw "hbao build failed: $LASTEXITCODE" } } finally { Pop-Location }
(Get-FileHash (Join-Path $out 'acu_hbao_probe.dll') -Algorithm SHA256).Hash.ToLowerInvariant()
# acu_tod.dll (tod_probe.cpp): time-of-day switch for live sun captures; no MinHook needed.
$line2 = 'call "' + $vc + '" >nul && cl /nologo /O2 /W3 /EHa /LD /std:c++17 "' + $src + '\tod_probe.cpp" /Fo:shader_probe_obj\ /Fe:acu_tod.dll /link /INCREMENTAL:NO user32.lib'
Push-Location $out
try { & cmd.exe /d /s /c $line2; if ($LASTEXITCODE) { throw "tod build failed: $LASTEXITCODE" } } finally { Pop-Location }
(Get-FileHash (Join-Path $out 'acu_tod.dll') -Algorithm SHA256).Hash.ToLowerInvariant()
