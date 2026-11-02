@echo off
setlocal
cd /d "%~dp0"
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "PATH=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer;%PATH%"
set "VSPATH="
if exist "%VSWHERE%" for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSPATH=%%i"
if not defined VSPATH set "VSPATH=C:\Program Files\Microsoft Visual Studio\2022\Community"
call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" 10.0.22000.0 >nul
if errorlevel 1 exit /b 1
if not exist bin mkdir bin
if not exist obj mkdir obj
cl /nologo /std:c++17 /EHsc /O2 /W4 /DUNICODE /D_UNICODE /DNOMINMAX overlay.cpp /Fo:obj\overlay.obj /Fe:bin\TrackpadOverlay.exe /link /SUBSYSTEM:WINDOWS d3d11.lib dxgi.lib dcomp.lib d2d1.lib dwmapi.lib windowsapp.lib windowscodecs.lib user32.lib gdi32.lib ole32.lib runtimeobject.lib shell32.lib
if errorlevel 1 exit /b 1
cl /nologo /std:c++17 /EHsc /O2 /W4 /DUNICODE /D_UNICODE /DNOMINMAX driver.cpp /Fo:obj\driver.obj /Fe:bin\OverlayDriver.exe /link user32.lib dwmapi.lib ole32.lib windowscodecs.lib gdi32.lib winmm.lib uiautomationcore.lib
exit /b %errorlevel%
