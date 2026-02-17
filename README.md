# Game Translator

ゲーム画面の英語テキストをリアルタイムでOCR認識し、日本語に翻訳してオーバーレイ表示するWindows向けツールです。

## 機能

- **Windows OCR** によるリアルタイムテキスト認識
- **DeepL API / Groq API / ローカルLLM** による翻訳（切り替え可能）
- **Direct2D オーバーレイ** によるゲーム画面上への翻訳表示
- **egui GUI** による直感的な設定・操作
- 翻訳キャッシュによる高速化
- 翻訳対象ウィンドウの選択機能

## 動作要件

- Windows 10 / 11
- Windows OCR 英語言語パック（設定 → 時刻と言語 → 言語と地域 → 英語(米国) を追加）
- 各種APIキー（使用する翻訳エンジンに応じて）
  - DeepL API キー
  - Groq API キー
  - ローカルLLMのエンドポイント（OpenAI互換）

## インストール

1. [Releases](https://github.com/Rangesa/game-translator/releases) ページから最新の `game_translator.zip` をダウンロード（予定）
2. 任意のフォルダに展開
3. `game_translator.exe` を実行

## ビルド方法

[Rust](https://www.rust-lang.org/tools/install) がインストールされている環境で:

```bash
git clone https://github.com/Rangesa/game-translator.git
cd game-translator
cargo build --release
```

ビルド成果物: `target/release/game_translator.exe`

## 使い方

1. `game_translator.exe` を起動
2. **翻訳設定** で翻訳エンジンを選択
   - **DeepL**: APIキーを入力
   - **Local LLM**: エンドポイントURL（例: `http://localhost:5000`）・モデル名を設定
   - **Groq**: APIキー・モデル名（例: `llama-3.3-70b-versatile`）を設定
3. **対象ウィンドウ** で翻訳したいゲームのウィンドウを選択し「更新」を押す
4. 「開始」ボタンを押す
5. ゲーム画面上に翻訳テキストがオーバーレイ表示される
6. 停止する場合は「停止」ボタンを押す

## 設定ファイル

初回起動後、`config.toml` が exe と同じフォルダに生成されます。GUIからも変更可能です。

```toml
translation_engine = "DeepL"     # "DeepL", "LocalLLM", "Groq"
deepl_api_key = ""               # DeepL APIキー
local_llm_endpoint = "http://localhost:5000"
local_llm_model = "default"
groq_api_key = ""                # Groq APIキー
groq_model = "llama-3.3-70b-versatile"
source_lang = "EN"               # 翻訳元言語
target_lang = "JA"               # 翻訳先言語
overlay_text_color = [1.0, 1.0, 0.0, 1.0]   # テキスト色 (RGBA)
overlay_bg_color = [0.0, 0.0, 0.0, 0.85]    # 背景色 (RGBA)
```

## フォントクレジット

GUI および オーバーレイ表示に [マキナス 4](https://moji-waku.com) フォントを使用しています。

- **Makinas 4 Square** — Moji-Waku Kenkyu (もじワク研究)
- ライセンス: フリーフォント（商用利用可）
- https://moji-waku.com/mj_work_license/

## ライセンス

MIT License — 詳細は [LICENSE](LICENSE) を参照してください。
