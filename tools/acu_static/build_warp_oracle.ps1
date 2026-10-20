param([Parameter(Mandatory=$true)][string]$Image)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$imagePath=(Resolve-Path -LiteralPath $Image).Path
if ((Get-FileHash -LiteralPath $imagePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne '9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520') { throw 'Unrecognized mapped ACU image' }
$out=Join-Path $root 'local/traversal-warper-oracle'
New-Item -ItemType Directory -Path $out -Force | Out-Null
$vc='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
$line='call "'+$vc+'" >nul && ml64 /nologo /c /Fo warp_thunk.obj "'+$PSScriptRoot+'\warp_oracle.asm" && cl /nologo /O2 /W4 /EHsc /std:c++17 "'+$PSScriptRoot+'\warp_oracle.cpp" warp_thunk.obj /Fe:warp_oracle.exe /link /INCREMENTAL:NO'
Push-Location $out
try {
    & cmd.exe /d /s /c $line
    if ($LASTEXITCODE) { throw 'Oracle build failed' }
    $result=& ./warp_oracle.exe $imagePath
    if ($LASTEXITCODE) { throw "Oracle failed: $LASTEXITCODE" }
    [IO.File]::WriteAllText((Join-Path $out 'cases.json'),($result -join "`n"),[Text.UTF8Encoding]::new($false))
    $metadata=@{image=$imagePath; image_sha256=(Get-FileHash -LiteralPath $imagePath).Hash.ToLowerInvariant(); oracle_sha256=(Get-FileHash ./warp_oracle.exe).Hash.ToLowerInvariant(); cases_sha256=(Get-FileHash ./cases.json).Hash.ToLowerInvariant(); source_sha256=@{}}
    foreach($name in @('warp_oracle.asm','warp_oracle.cpp')) { $metadata.source_sha256[$name]=(Get-FileHash (Join-Path $PSScriptRoot $name)).Hash.ToLowerInvariant() }
    $metadata | ConvertTo-Json -Depth 4 | Set-Content manifest.json
    Write-Output (Join-Path $out 'cases.json')
} finally { Pop-Location }
