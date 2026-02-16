@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ===========================================
echo   TabbyAPI 自動セットアップ
echo   Game Translator ローカル翻訳バックエンド
echo ===========================================
echo.

set "BASE_DIR=E:\Programming\AI"
set "TABBY_DIR=%BASE_DIR%\tabbyAPI"
set "MODEL_NAME=google/translategemma-4b-it"
set "MODEL_DIR=%BASE_DIR%\models"
set "MODEL_LOCAL=%MODEL_DIR%\translategemma-4b-it"

REM ============================================
REM [1/4] 前提条件チェック
REM ============================================
echo [1/4] 前提条件チェック...

where git >nul 2>&1
if %errorlevel% neq 0 goto :err_no_git
echo   Git: OK
echo.

REM ============================================
REM [2/4] TabbyAPI クローン
REM ============================================
echo [2/4] TabbyAPI...

if exist "%TABBY_DIR%\.git" (
    echo   既存: %TABBY_DIR%
    cd /d "%TABBY_DIR%"
    git pull --quiet 2>nul
) else (
    echo   クローン中...
    cd /d "%BASE_DIR%"
    git clone https://github.com/theroyallab/tabbyAPI
    if !errorlevel! neq 0 goto :err_clone_tabby
)
echo   OK
echo.

REM ============================================
REM [3/4] モデルダウンロード
REM ============================================
echo [3/4] TranslateGemma ダウンロード...

if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%"

if exist "%MODEL_LOCAL%\config.json" (
    echo   既にダウンロード済み
    goto :step4
)

echo   モデル: %MODEL_NAME%
echo   保存先: %MODEL_LOCAL%
echo   ダウンロード開始 (git clone)...

cd /d "%MODEL_DIR%"
git clone https://huggingface.co/%MODEL_NAME% translategemma-4b-it
if !errorlevel! neq 0 goto :err_download

echo   OK

:step4
echo.

REM ============================================
REM [4/4] TabbyAPI 設定
REM ============================================
echo [4/4] TabbyAPI 設定ファイル生成...

cd /d "%TABBY_DIR%"

> "%TABBY_DIR%\config.yml" (
    echo # Game Translator - auto generated
    echo model:
    echo   model_dir: %MODEL_DIR%
    echo   model_name: translategemma-4b-it
    echo.
    echo network:
    echo   host: 0.0.0.0
    echo   port: 5000
    echo   disable_auth: true
    echo.
    echo developer:
    echo   gpu_split_auto: true
)
echo   config.yml OK

> "%TABBY_DIR%\api_tokens.yml" (
    echo api_key: "x"
    echo admin_key: "x"
)
echo   api_tokens.yml OK
echo.

REM ============================================
REM 起動
REM ============================================
echo ===========================================
echo   セットアップ完了!
echo ===========================================
echo.
echo   TabbyAPI : %TABBY_DIR%
echo   モデル   : %MODEL_LOCAL% (FP16 - 量子化不要)
echo   ポート   : 5000
echo.
echo   初回起動時:
echo     - 依存関係が自動インストールされます
echo     - GPU選択で CUDA 12.x を選んでください
echo.

set /p "LAUNCH=今すぐTabbyAPIを起動しますか? (Y/n): "
if /i "!LAUNCH!"=="n" goto :done

echo.
echo   TabbyAPI 起動中...
cd /d "%TABBY_DIR%"
call start.bat
goto :done

REM ============================================
REM エラーハンドラ
REM ============================================
:err_no_git
echo   [ERROR] Gitが見つかりません
pause
exit /b 1

:err_clone_tabby
echo   [ERROR] TabbyAPIのクローンに失敗
pause
exit /b 1

:err_download
echo.
echo   [ERROR] モデルダウンロード失敗
echo.
echo   対処法:
echo     1. https://huggingface.co/google/translategemma-4b-it でライセンスに同意
echo     2. HF認証を確認
echo     3. 手動: git clone https://huggingface.co/google/translategemma-4b-it
echo     4. %MODEL_LOCAL% に配置して再実行
echo.
pause
exit /b 1

:done
echo.
pause
