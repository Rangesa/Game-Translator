# Game Translator

ゲーム画面の英語テキストをリアルタイムでOCR認識し、日本語に翻訳してオーバーレイ表示するWindows向けツールです。

## 機能

- **Windows OCR** によるリアルタイムテキスト認識
- **DeepL API / ローカルLLM** による翻訳（切り替え可能）
- **Direct2D オーバーレイ** によるゲーム画面上への翻訳表示
- **egui GUI** による直感的な操作
- 翻訳キャッシュによる高速化
- 対象ウィンドウの選択機能

## 動作要件

- Windows 10 / 11
- Windows OCR 英語言語パック（設定 → 時刻と言語 → 言語と地域 → 英語(米国) を追加）
- DeepL API キー（DeepL翻訳を使用する場合）

## インストール

1. [Releases](https://github.com/Rangesa/game-translator/releases) ページから最新の `game_translator.zip` をダウンロード
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
2. 翻訳エンジンを選択（DeepL または ローカルLLM）
   - DeepL: APIキーを入力
   - ローカルLLM: エンドポイントURL・モデル名を設定
3. 「ウィンドウ一覧を更新」で対象ウィンドウを取得
4. 翻訳したいゲームのウィンドウを選択
5. 「翻訳開始」を押す
6. ゲーム画面上に翻訳テキストがオーバーレイ表示される
7. 停止する場合は「翻訳停止」を押す

## 設定ファイル

初回起動後、`config.toml` が exe と同じフォルダに生成されます。

```toml
translation_engine = "DeepL"     # "DeepL" または "LocalLLM"
deepl_api_key = ""               # DeepL APIキー
local_llm_endpoint = "http://localhost:5000"  # ローカルLLMのエンドポイント
local_llm_model = "default"      # モデル名
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
