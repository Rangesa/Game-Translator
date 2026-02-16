# ExLlamaV3 + TabbyAPI + TranslateGemma セットアップガイド

Game Translatorのローカル翻訳バックエンド用。

## 概要

- **推論エンジン**: ExLlamaV3 (TabbyAPI経由)
- **モデル**: google/translategemma-4b-it
- **API**: OpenAI互換 (http://localhost:5000)
- **用途**: ゲーム画面のEN→JA リアルタイム翻訳

## 前提条件

- Python 3.10, 3.11, or 3.12
- CUDA対応GPU (VRAM 6GB以上推奨)
- Git

## 1. TabbyAPI のインストール

TabbyAPIはExLlamaV3およびEXL3形式をサポートしています。

```bash
git clone https://github.com/theroyallab/tabbyAPI
cd tabbyAPI
```

### Windows
```bash
start.bat
```

### Linux
```bash
./start.sh
```

初回起動時に依存関係が自動インストールされます。

## 2. TranslateGemma モデルの取得

### 方法A: HuggingFaceから直接 (EXL3形式)

EXL3形式の量子化済みモデルをHuggingFaceで探します:

```
https://huggingface.co/models?search=translategemma+exl3
```

### 方法B: 自分で量子化 (EXL3)

ExLlamaV3のリポジトリを使用して量子化を行います。

1. **ExLlamaV3のクローンとインストール**
   ```bash
   git clone https://github.com/turboderp-org/exllamav3
   cd exllamav3
   pip install -r requirements.txt
   ```

2. **モデルの変換 (EXL3)**
   ```bash
   # 作業用ディレクトリ(work_dir)が必要です
   mkdir work_dir
   
   python convert.py \
     -i ../google/translategemma-4b-it \
     -o ../translategemma-4b-it-exl3 \
     -w work_dir \
     -b 4.0
   ```
   
   ※ `-i`: 入力モデル(HF形式)のパス
   ※ `-o`: 出力先パス
   ※ `-w`: 一時作業用ディレクトリ
   ※ `-b`: ビットレート (bpw)

## 3. TabbyAPI の設定

`config_sample.yml` をコピーして `config.yml` を作成:

```bash
cp config_sample.yml config.yml
```

`config.yml` を編集:

```yaml
# モデル設定
model:
  model_dir: /path/to/models  # モデルのあるディレクトリ (親ディレクトリ)
  model_name: translategemma-4b-it-exl3  # モデルフォルダ名

# サーバー設定
network:
  host: 0.0.0.0
  port: 5000

# GPU設定
developer:
  gpu_split_auto: true
```

### API トークン設定

`api_tokens_sample.yml` をコピー:
```bash
cp api_tokens_sample.yml api_tokens.yml
```

`api_tokens.yml` を編集（認証不要にする場合は空にするか、トークンを設定）:
```yaml
# 開発用: 認証なし
api_tokens: []
```

## 4. サーバー起動

```bash
# Windows
start.bat

# Linux
./start.sh
```

起動成功時のログ:
```
INFO: Uvicorn running on http://0.0.0.0:5000
```

## 5. 動作確認

```bash
curl http://localhost:5000/v1/models
```

レスポンスにモデル名が表示されればOK。

翻訳テスト:
```bash
curl http://localhost:5000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "translategemma-4b-it-exl3",
    "messages": [{"role": "user", "content": "Translate from English to Japanese: Hello World"}],
    "temperature": 0.1,
    "max_tokens": 256
  }'
```

## 6. Game Translator との接続

Game Translator起動時:
1. 翻訳バックエンドで `[2] ローカルLLM` を選択
2. デフォルトで `http://localhost:5000` に接続

### エンドポイント・モデル名の変更

`game-translator/src/main.rs` の定数を変更:
```rust
const LOCAL_LLM_ENDPOINT: &str = "http://localhost:5000";
const LOCAL_LLM_MODEL: &str = "translategemma-4b-it-exl3";
```

変更後: `cargo build --release`

## TranslateGemma プロンプト形式

TranslateGemmaは独自のチャットテンプレートを持ちますが、TabbyAPIが適切に処理するか、またはGame Translator側で調整します。基本的なOpenAI互換リクエストで動作します。

## トラブルシューティング

### CUDA out of memory
- EXL3の量子化ビット数を下げる (4.0 → 3.0)
- `gpu_split_auto: true` を確認

### モデルが見つからない
- `config.yml` の `model_dir` と `model_name` を確認
- モデルフォルダ内に `config.json` (または `measurement.json` 等、EXL3の構成ファイル) が存在するか確認

### API接続エラー
- TabbyAPIが起動しているか確認: `curl http://localhost:5000/v1/models`
- ファイアウォールでポート5000がブロックされていないか確認

## 参考リンク

- TabbyAPI: https://github.com/theroyallab/tabbyAPI
- ExLlamaV3: https://github.com/turboderp-org/exllamav3
- TranslateGemma: https://huggingface.co/google/translategemma-4b-it
- TranslateGemma GGUF (参考): https://huggingface.co/SandLogicTechnologies/translategemma-4b-it-GGUF
