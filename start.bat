@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ===========================================
echo   Game Translator - Launcher
echo ===========================================
echo.

set "TABBY_DIR=E:\Programming\AI\tabbyAPI"
set "GAME_EXE=E:\Programming\AI\game-translator\target\release\game_translator.exe"
set "LLM_ENDPOINT=http://localhost:5000"

REM ============================================
REM TabbyAPI 起動
REM ============================================

REM 既に起動してるか確認
curl -s "%LLM_ENDPOINT%/v1/models" >nul 2>&1
if %errorlevel% equ 0 (
    echo   TabbyAPI: 既に起動済み
    goto :start_game
)

echo   TabbyAPI 起動中...
start /MIN "TabbyAPI" cmd /C "cd /d %TABBY_DIR% && start.bat"

REM サーバー応答待ち (最大120秒)
echo   モデル読み込み待機中
set /a count=0
:wait_loop
if !count! geq 120 goto :err_timeout
curl -s "%LLM_ENDPOINT%/v1/models" >nul 2>&1
if %errorlevel% equ 0 goto :server_ready
set /a count+=1
<nul set /p "=."
timeout /t 1 /nobreak >nul
goto :wait_loop

:server_ready
echo.
echo   TabbyAPI: 起動完了 (%count%秒)
echo.

REM ============================================
REM Game Translator 起動
REM ============================================
:start_game
if not exist "%GAME_EXE%" (
    echo   [ERROR] game_translator.exe が見つかりません
    echo   先に cargo build --release を実行してください
    pause
    exit /b 1
)

echo   Game Translator 起動...
echo.
"%GAME_EXE%"
goto :done

:err_timeout
echo.
echo   [ERROR] TabbyAPI起動タイムアウト (120秒)
echo   %TABBY_DIR%\start.bat を手動で起動してください
pause
exit /b 1

:done
pause
