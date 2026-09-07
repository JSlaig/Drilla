@echo off
setlocal

set APP_NAME=drilla
set INSTALL_DIR=%LOCALAPPDATA%\Programs\%APP_NAME%
set EXE=%INSTALL_DIR%\%APP_NAME%.exe

echo Installing Drilla (SSH Tunnel CLI) to %INSTALL_DIR%

if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"

where ssh >nul 2>nul
if %errorlevel% neq 0 (
    echo ERROR: OpenSSH client 'ssh' was not found on PATH.
    echo It ships with Windows 10/11 - you can enable it in:
    echo   Settings ^> Apps ^> Optional features ^> OpenSSH Client
    exit /b 1
)

copy /y "%~dp0drilla.exe" "%EXE%" >nul
if %errorlevel% neq 0 (
    echo ERROR: failed to copy the executable.
    exit /b 1
)

REM Install the "drll" alias launcher alongside drilla.exe.
copy /y "%~dp0drll.cmd" "%INSTALL_DIR%\drll.cmd" >nul

REM Add install dir to the user PATH (permanent) if not already there,
REM so "drilla" and "drll" work from any new terminal.
echo %PATH% | findstr /i "%INSTALL_DIR%" >nul
if %errorlevel% neq 0 (
    setx PATH "%INSTALL_DIR%;%PATH%" >nul
)

echo.
echo Installed. Open a NEW terminal and run:  drilla   (or its alias:  drll)
echo.
echo Note: your tunnel config lives at %%USERPROFILE%%\.ssh\tunnels.json

endlocal