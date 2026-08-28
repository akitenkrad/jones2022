[English](cli.md) | **日本語**

# CLI

2 つのコマンドライン面: Rust バイナリ `jones`（プローブパイプライン）と Python `jones-tools`（可視化）．

## Rust `jones`

`cargo build --release` でビルドし，バイナリは `target/release/jones`．サブコマンドは `run`・`sweep`・`reproduce` の 3 つで，いずれも常にコンパイルされ，フィーチャフラグは無い．手法全体が **ブラックボックスかつ logprob 非依存** で，トークン確率を要求せず生成テキストのみを読む．`--mock` は全パイプラインを決定論的スクリプトクライアントでオフライン実行する．付けない場合，生成はプロンプトキャッシュ付きの **Ollama→OpenAI フォールバック** へ向かう．

### ローカル Ollama 設定

```bash
# 1. Ollama（https://ollama.com）を導入・起動し，コードモデルを pull:
ollama pull codellama        # または qwen2.5-coder / deepseek-coder / starcoder2

# 2. Ollama は既定で http://localhost:11434 を待ち受ける（--ollama-host または
#    OLLAMA_HOST 環境変数で上書き）．Ollama 到達不能時は OpenAI フォールバック
#    （例 gpt-4o-mini）を使う；OPENAI_API_KEY（必要なら OPENAI_MODEL）を設定する．
```

ライブのモデル名は `--model` で渡し，ハーネスは `OLLAMA_MODEL` / `OPENAI_MODEL` 経由で読む．生成は greedy（`temperature = 0`）で seed を固定し，completion は `<results>/llm_cache.json` にキャッシュされるため再実行は同一テキストを再生する．

### `run` — 単一実験（E1–E7）

1 つの認知バイアス実験を実行・採点し，`metrics.csv` + `config.json` を書き出す．

```bash
# HumanEval 上の E1 framing（オフラインスクリプト mock）
cargo run --release -- run --experiment framing --mock --seed 42

# E7 ファイル削除，3 パッケージ，ライブモデル
cargo run --release -- run --experiment file-deletion --model codellama --num-packages 3
```

| フラグ | 既定 | 意味 |
|---|---|---|
| `--experiment <E>` | `framing` | `framing` \| `anchoring` \| `availability` \| `attribute-substitution` \| `gpt3-anchoring` \| `gpt3-framing` \| `file-deletion`（別名 `e1`…`e7`） |
| `--model <MODEL>` | `codellama` | ライブのモデル名（Ollama コードモデル；OpenAI フォールバック） |
| `--seed <SEED>` | `42` | 生成 seed |
| `--mock` | `false` | ライブモデルの代わりに決定論的スクリプトクライアントを使う（オフライン） |
| `--results <DIR>` | `results` | 結果ルートディレクトリ |
| `--dataset <PATH>` | — | HumanEval JSONL パス（E1/E2）；省略でバンドル subset |
| `--full` | `false` | `data/HumanEval.jsonl` があればフルセットを優先（E1/E2） |
| `--limit <N>` | `0` | 先頭 N 問のみ評価（0 = 全件）；スコープ付きライブスモーク用 |
| `--math-count <N>` | `8` | E3/E4 MathEquations の問題数；curate 済み8問が安定 prefix，超過分は生成 |
| `--math-seed <S>` | `42` | curate 済み8問を超える MathEquations 生成のシード（E3/E4） |
| `--anchor-ratio <p>` | `0.5` | E5 アンカー比率（高 = a(1+p)，低 = a(1−p)） |
| `--respondents <N>` | `10` | E6 フレーミング回答者数 |
| `--num-packages <N>` | `3` | E7「アンインストール」パッケージ数 |
| `--trials <N>` | `8` | E7 削除試行数 |
| `--sandbox <tempdir\|container>` | `tempdir` | E7 サンドボックスバックエンド（`tempdir` 実装済；`container` は予約） |
| `--ollama-host <URL>` | — | `OLLAMA_HOST` を上書き（グローバルフラグ） |

出力（`results/{stamp}/` 下）: `config.json` と `metrics.csv`（long: `experiment, variant, condition, n, functional_accuracy, indicator_rate, delta`）．コード実験（E1–E4）では `baseline` 行が機能的正解率を，`transform` 行が正解率 + 指標 + `Δ` を持つ．E5/E6/E7 では正解率は `NaN` で，信号は指標率である．

#### MathEquations — 生成セット（E3/E4）

HumanEval と違い，論文の MathEquations は著者独自・非公開のため，再現実装では**プログラム生成**する．`--math-count`/`--math-seed` で決定論的にスケールでき，curate 済み8問が安定 prefix，超過分は演算子優先順位テンプレート（例: `(x + y) * k` と素朴な誤読 `x + y * k`）から生成する．生成問題もデータセットの不変条件を保つ — `distractor_token` は誤答本体にのみ現れ，単体テストは **2 つの解釈が食い違う入力で canonical 値**を assert するので，バイアス補完は必ずテストに落ちる．実際の `n` とシードは `config.json`（`dataset: "math_equations(n=…, seed=…)"`）に記録される．`--math-count 8`（既定）は元の curate 済みセットをそのまま再現する．

```bash
cargo run -p jones-simulation -- run --experiment availability --math-count 90 --math-seed 7
```

### `sweep` — パラメータ掃引

実験をパラメータグリッドで実行しサマリを書き出す．パラメータ化された実験は 2 つ: **E7**（パッケージ数）と **E5**（アンカー比率）．

```bash
cargo run --release -- sweep --experiment file-deletion  --num-packages-values 1,2,3,4,5,6
cargo run --release -- sweep --experiment gpt3-anchoring --anchor-ratio-values 0.1,0.2,0.5,0.8
```

| フラグ | 既定 | 意味 |
|---|---|---|
| `--experiment <E>` | `file-deletion` | `file-deletion`（num-packages）または `gpt3-anchoring`（anchor-ratio） |
| `--mock` | `false` | スクリプトクライアントを使う（オフライン） |
| `--model <MODEL>` | `codellama` | ライブのモデル名 |
| `--seed <SEED>` | `42` | 生成 seed |
| `--results <DIR>` | `results` | 結果ルートディレクトリ |
| `--num-packages-values <list>` | `1,2,3,4,5,6` | E7 パッケージ数（カンマ区切り） |
| `--anchor-ratio-values <list>` | `0.1,0.2,0.5,0.8` | E5 アンカー比率（カンマ区切り） |
| `--trials <N>` | `8` | E7 パッケージ数あたりの削除試行数 |

出力（`results/sweep_{stamp}/` 下）: `sweep_config.json` と `sweep_summary.csv`（long: `experiment, param, value, variant, functional_accuracy, indicator_rate, delta`）．

### `reproduce` — オフライン論文アンカー照合

組み込みの `PAPER_ANCHORS`（論文の E1–E7 参照値）を観測指標と突き合わせ，各アンカーを分類する．

```bash
# オフライン: 全実験を mock で実行してから分類
cargo run --release -- reproduce --mock

# 実 run の metrics.csv から（results/latest を生成した実験）
cargo run --release -- reproduce
```

| フラグ | 既定 | 意味 |
|---|---|---|
| `--results <DIR>` | `results` | 観測指標を読む結果ルート（`{DIR}/latest/metrics.csv`） |
| `--mock` | `false` | 全実験を mock で実行して観測値をオフライン生成 |

データ欠損で panic することはない: 観測の無いアンカーは `NO_DATA` になる．出力は `paper_anchors.csv`（参照値）と `reproduce_summary.csv`（観測 vs 論文，`status ∈ {PASS, off, NO_DATA}`）で，`PASS / off / NO_DATA` の集計を表示する．`--mock` はスタブのため，その «PASS» は偶発的である — 真の一致はライブモデルでのみ得られる．

## Python `jones-tools`

ワークスペースルートで `uv sync` し，`uv run jones-tools <subcommand>` で起動する．CLI のディスパッチ先: `visualize`・`visualize-sweep`・`show-experiment-settings`・`reproduce-paper`・`fetch-dataset`．図系サブコマンドは結果ディレクトリを位置引数またはフラグで取る（既定 `results/latest`）．

```bash
uv run jones-tools visualize results/latest                 # または --results-dir DIR
uv run jones-tools visualize-sweep results/sweep_20260101_120000   # または --sweep-dir DIR
uv run jones-tools show-experiment-settings results/latest  # または --results-dir DIR
uv run jones-tools reproduce-paper results/latest           # または --results-dir / --summary-csv
uv run jones-tools fetch-dataset                            # 公式 HumanEval 164 問を取得
```

| サブコマンド | 役割 | 入力 | 出力 |
|---|---|---|---|
| `visualize` | 単一 run の図（実験種別を自動判定） | `metrics.csv` + `config.json` | `fig_accuracy.png`（E1–E4），`fig_indicator.png` |
| `visualize-sweep` | 掃引パラメータ × 指標（と Δ） | `sweep_summary.csv` + `sweep_config.json` | `fig_sweep.png` |
| `show-experiment-settings` | run 設定の整形表示 | `config.json` | —（コンソール） |
| `reproduce-paper` | アンカー別 観測 vs 論文 の図 | `reproduce_summary.csv` | `reproduce_paper.png` |
| `fetch-dataset` | 公式 HumanEval セットの取得 | ネットワーク（`openai/human-eval`, MIT） | `simulation/data/HumanEval.jsonl` |

図はヘッドレス（Agg）バックエンドを用いるため，ディスプレイ無しの CI でも描画される．[可視化](visualization.ja.md) を参照．

### `fetch-dataset` — 公式 HumanEval セット

本リポジトリは完全オフラインで動くよう，curate した 8 問サブセットのみを同梱している．`fetch-dataset` はフルの **164 問** セット（`openai/human-eval`, MIT ライセンス）をダウンロードし `simulation/data/HumanEval.jsonl` に書き出す．これは Rust ローダが自動検出するパスなので，取得後は `jones run ... --full` でそのまま 164 問を実行できる（追加配線不要）．取得できるのは HumanEval のみで，論文の MathEquations は著者独自・非公開（再現実装ではコード合成）．

```bash
uv run jones-tools fetch-dataset            # → simulation/data/HumanEval.jsonl（164 問）
cargo run -p jones-simulation -- run --experiment framing --full
```

| フラグ | 既定 | 意味 |
|---|---|---|
| `--output <PATH>` | `<repo>/simulation/data/HumanEval.jsonl` | JSONL の保存先 |
| `--url <URL>` | `openai/human-eval` の `HumanEval.jsonl.gz` raw URL | 取得元アーカイブ（gzip 圧縮 JSONL） |
| `--force` | `false` | 妥当なファイルが既にあっても再取得 |

**冪等**（妥当な 164 問ファイルがあれば skip），**アトミック書き込み**（一時ファイル→rename．中断しても壊れた本体を残さない），ピン留めした SHA-256 と 164 問スキーマで**検証**（不一致なら部分保存せず明示エラー），URL・ライセンス・バイト数・SHA-256・問題数を表示する．ダウンロードしたフルセットは git 管理外（追跡されるのは 8 問サブセットのみ）．取得は常に明示的な別ステップで，`run` から暗黙取得はしない（offline-first）．

## 結果レイアウト

```
results/
├── latest -> 20260101_120000/
├── llm_cache.json                  # ライブ run: prompt→completion キャッシュ
├── 20260101_120000/                # run
│   ├── config.json
│   ├── metrics.csv                 # experiment × variant × condition → acc / r / Δ
│   ├── fig_accuracy.png            # visualize（E1–E4）
│   └── fig_indicator.png           # visualize
├── sweep_20260101_120100/          # sweep
│   ├── sweep_config.json
│   ├── sweep_summary.csv           # param × variant → acc / r / Δ
│   └── fig_sweep.png               # visualize-sweep
└── 20260101_120200/                # reproduce
    ├── paper_anchors.csv
    ├── reproduce_summary.csv        # 論文 vs 観測，PASS/off/NO_DATA
    └── reproduce_paper.png          # reproduce-paper
```
