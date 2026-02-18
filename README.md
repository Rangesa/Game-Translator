# Game Translator

ゲーム画面の英語テキストをリアルタイムでOCR認識し、日本語に翻訳してオーバーレイ表示するWindows向けツールです。

## 機能

- **Windows OCR** によるリアルタイムテキスト認識
- **DeepL API / Groq API / ローカルLLM** による翻訳（切り替え可能）
- **Direct2D オーバーレイ** によるゲーム画面上への翻訳表示
- **egui GUI** による直感的な設定・操作
- 翻訳キャッシュによる高速化
- 翻訳対象ウィンドウの選択機能

## 翻訳エンジンの選択について

本ツールは複数の翻訳方式を試した結果として、現在の構成に至っています。それぞれの方式にトレードオフがあるため、用途に応じて使い分けることを推奨します。

### DeepL API（推奨）

精度という観点では **DeepL API が現時点での最適解** です。ゲームテキストのような断片的な英語文に対しても高品質な翻訳を返してくれます。

ただし、DeepL には利用制限があります。

- **無料プラン**: 月 500,000 文字までの翻訳制限あり
- **有料プラン (Pro)**: 従量課金制で制限が緩和されるが、コストが発生する

長時間プレイや翻訳量が多い場合には上限に達する可能性があるため、完全に自由に使えるわけではない点に注意してください。

### ローカルLLM（オフライン・プライバシー重視）

TabbyAPI + TranslateGemma などのローカルLLMを使うことで、APIキー不要・完全オフラインでの翻訳が可能です。詳細は [docs/exllama3-setup.md](docs/exllama3-setup.md) を参照してください。

ただし、**タイムラグが発生する**という避けがたい課題があります。

- ローカルでモデルを推論するためGPUの性能に翻訳速度が依存する
- リアルタイム性が求められるゲーム翻訳用途では体感的な遅延が気になる場面がある
- 6GB以上のVRAMを搭載したNVIDIA GPU（CUDA対応）が必要

### Groq API（高速クラウドLLM）

DeepL の制限回避やローカルLLMの遅延問題への現実的な対処として、**Groq のような高速推論サービス** が有効です。`llama-3.3-70b-versatile` などの大規模言語モデルをクラウド上で高速実行でき、無料枠の範囲内であれば十分実用的です。

ただし、クラウドサービスへの依存という点ではDeepLと同様の制約があります。完全な自由度を求めるなら、将来的により高性能なローカルLLMが普及することを待つ必要があります。

### まとめ

| エンジン | 翻訳精度 | 速度 | コスト | オフライン |
|----------|----------|------|--------|----------|
| DeepL API | ★★★★★ | 速い | 無料枠あり（制限あり） | 不可 |
| Groq API  | ★★★★☆ | 速い | 無料枠あり | 不可 |
| ローカルLLM | ★★★☆☆ | 遅め（GPU依存） | 無料 | 可能 |

---

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
