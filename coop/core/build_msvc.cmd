@echo off
rem Build and run the acoop tests with MSVC for x86 (the AC4 mod's target) and x64.
rem Usage: build_msvc.cmd <output-dir>
setlocal
set OUT=%~1
if "%OUT%"=="" set OUT=%~dp0build
if not exist "%OUT%" mkdir "%OUT%"
set VCVARS=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat
for %%A in (x86 x64) do (
  call "%VCVARS%" %%A >nul || exit /b 1
  cl /nologo /std:c++17 /EHsc /W4 /WX /permissive- /O2 /I"%~dp0include" "%~dp0tests\test_session.cpp" /Fe"%OUT%\test_msvc_%%A.exe" /Fo"%OUT%\\" || exit /b 1
  "%OUT%\test_msvc_%%A.exe" || exit /b 1
)
echo MSVC x86 and x64: all tests passed
