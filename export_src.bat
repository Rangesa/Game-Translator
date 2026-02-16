@echo off
set OUTPUT_FILE=game_translator_all_src.txt

echo Exporting source code to %OUTPUT_FILE%...

:: Clear the output file if it exists
echo. > %OUTPUT_FILE%

:: Export Cargo.toml
echo --- Cargo.toml --- >> %OUTPUT_FILE%
type Cargo.toml >> %OUTPUT_FILE%
echo. >> %OUTPUT_FILE%
echo. >> %OUTPUT_FILE%

:: Export README.md
if exist README.md (
    echo --- README.md --- >> %OUTPUT_FILE%
    type README.md >> %OUTPUT_FILE%
    echo. >> %OUTPUT_FILE%
    echo. >> %OUTPUT_FILE%
)

:: Export all .rs files in src directory
for /r src %%f in (*.rs) do (
    echo --- src/%%~nxf --- >> %OUTPUT_FILE%
    type "%%f" >> %OUTPUT_FILE%
    echo. >> %OUTPUT_FILE%
    echo. >> %OUTPUT_FILE%
)

echo Done! Source code has been saved to %OUTPUT_FILE%.
pause
