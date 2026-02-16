@echo off
setlocal enabledelayedexpansion

:menu
cls
echo ===========================================
echo  Source Code Exporter
echo ===========================================
echo  1. Single Text File (All-in-one)
echo  2. Multiple Text Files (Individual copies)
echo  3. Exit
echo ===========================================
set /p choice="Select an option (1-3): "

if "%choice%"=="1" goto single_file
if "%choice%"=="2" goto multi_file
if "%choice%"=="3" exit
goto menu

:single_file
set OUTPUT_FILE=game_translator_all_src.txt
echo Exporting to %OUTPUT_FILE%...
echo. > "%OUTPUT_FILE%"

echo --- Cargo.toml --- >> "%OUTPUT_FILE%"
type Cargo.toml >> "%OUTPUT_FILE%"
echo. >> "%OUTPUT_FILE%"

if exist README.md (
    echo --- README.md --- >> "%OUTPUT_FILE%"
    type README.md >> "%OUTPUT_FILE%"
    echo. >> "%OUTPUT_FILE%"
)

for /r src %%f in (*.rs) do (
    echo --- src/%%~nxf --- >> "%OUTPUT_FILE%"
    type "%%f" >> "%OUTPUT_FILE%"
    echo. >> "%OUTPUT_FILE%"
)
echo Done! Saved to %OUTPUT_FILE%
pause
goto menu

:multi_file
set EXPORT_DIR=exported_src_txt
if not exist "%EXPORT_DIR%" mkdir "%EXPORT_DIR%"
echo Exporting files to %EXPORT_DIR% folder...

copy Cargo.toml "%EXPORT_DIR%\Cargo.toml.txt" > nul
if exist README.md copy README.md "%EXPORT_DIR%\README.md.txt" > nul

for /r src %%f in (*.rs) do (
    copy "%%f" "%EXPORT_DIR%\%%~nxf.txt" > nul
)
echo Done! Files are in the "%EXPORT_DIR%" directory.
pause
goto menu
