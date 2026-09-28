[English](README.md) | **日本語**

# 人間の認知バイアスによる LLM の失敗誘発 — Jones & Steinhardt (2022)

Jones & Steinhardt (2022)「Capturing Failures of Large Language Models via Human Cognitive Biases」（[arXiv:2202.12299](https://arxiv.org/abs/2202.12299)）の再現実装．本論文は **人間の認知バイアス** を，LLM の質的失敗を系統的に誘発するレシピへと転用する: 失敗様式を仮説化し，それを誘発する **意味保存変換** を構成して，(a) 変換が機能的正解率を下げるか（感度 `Δ`），(b) 出力が標的失敗特徴を含むか（指標率 `r`）の 2 点を測る．変換は完全に **ブラックボックスかつ logprob 非依存** — モデルのテキスト出力のみを読み，先頭トークン確率を一切見ない — ため任意のモデルで成立する．原論文は Codex を用いたが，そのモデル（`davinci-001`）は廃止済みのため，本再現では現代のコードモデル（Ollama `codellama` / `qwen2.5-coder` / `deepseek-coder` / `starcoder2`，OpenAI `gpt-4o-mini` フォールバック）で代替し，**失敗の方向の保存** を再現目標とする．

本リポジトリは集約済み **socsim** ライブラリ上に薄く実装する．LLM 生成は `socsim-llm`，平均・率の集計は `socsim-metrics`，論文アンカー PASS/off 照合は `socsim-reproduce`，実行の記録 — ディレクトリ・その名前・`config.json`・指標・イベント・論文の報告値 — は [`runvault`](https://github.com/akitenkrad/rs-runvault) に委譲する．jones2022 固有は «意味保存変換・失敗指標 `φ`・コード実行／削除サンドボックス»（Rust `simulation/`）と Python 分析ツール（`tools/`）のみ．**プローブ + 指標パイプライン** であり ABM の tick loop ではないため，`socsim-core`/engine/grid/net は引かない．

## スコープ

論文の分析は実験 **E1–E7** に対応する:

| 実験 | 検証内容 | 主指標 | CLI |
|------|---------|--------|-----|
| **E1** フレーミング | 無関係前置関数 (IPF) の前置で本体を逐語コピーするか | `Δ`・逐語コピー率 `r` | `jones run --experiment framing` |
| **E2** アンカリング | アンカー関数の前置で distractor パターンに引き寄せられるか | アンカー行出現率 `r` | `jones run --experiment anchoring` |
| **E3** 利用可能性 | 演算順序反転で「想起しやすい」素朴解に誘導されるか | `Δ`・unary-first 率 `r` | `jones run --experiment availability` |
| **E4** 属性置換 | 矛盾する関数名が仕様の挙動を上書きするか | `Δ`・named-function 率 `r` | `jones run --experiment attribute-substitution` |
| **E5** GPT-3 アンカリング | 数値推定が上下アンカー `a(1±p)` 方向へ動くか | アンカー方向更新率・gibberish 率 | `jones run --experiment gpt3-anchoring` |
| **E6** GPT-3 フレーミング | Asian-Disease の save/die フレームでリスク選択率が変わるか | フレーム別リスク選択率 | `jones run --experiment gpt3-framing` |
| **E7** ファイル削除 | 「N パッケージ削除」要求で無関係ファイルを誤削除するか | 誤削除率 | `jones run --experiment file-deletion` |

E1/E2 は **HumanEval**（Chen et al. 2021），E3/E4 は小規模 **MathEquations** を用い，生成コードをサンドボックスで単体テスト実行して機能的正解率を測る．E5/E6 は数値・選択タスク（実行なし）．E7 は生成された「アンインストール」コードを **削除ガード**（全削除操作をフックし対象を記録するだけで実削除せず，使い捨て tempdir 内で実行）下で走らせるため，ホスト FS には一切触れない．詳細は [実験一覧](docs/studies.ja.md) を参照．

## インストールと実行

全コマンドは本ディレクトリから実行する．ライブ生成にはローカルの **Ollama**（`http://localhost:11434`，コードモデル pull 済み: `ollama pull codellama`）か，フォールバック用の OpenAI キーが必要．`--mock` はバンドル subset と決定論的スクリプトクライアントで全経路をオフライン実行する．

```bash
# Rust シミュレーションをビルド（バイナリ: jones）
cargo build --release

# === オフラインスモーク — 任意の実験，モデル不要 ===
cargo run --release -- run --experiment framing --mock --seed 42
cargo run --release -- run --experiment availability --mock
cargo run --release -- run --experiment gpt3-framing --mock
cargo run --release -- run --experiment file-deletion --mock --num-packages 3

# === sweep — E7 パッケージ数 / E5 アンカー比率 ===
cargo run --release -- sweep --experiment file-deletion  --mock --num-packages-values 1,2,3,4,5
cargo run --release -- sweep --experiment gpt3-anchoring --mock --anchor-ratio-values 0.1,0.2,0.5,0.8

# === ライブ実行（Ollama コードモデル）===
cargo run --release -- run --experiment framing --model codellama --seed 42

# === reproduce — オフライン論文アンカー PASS/off 照合（E1–E7 全アンカー）===
cargo run --release -- reproduce --mock

# === Python ツール ===
uv sync
uv run jones-tools visualize
uv run jones-tools reproduce-paper
```

HumanEval は小規模な **バンドル 8 問 subset** を同梱しており全経路がオフラインで走る．フル 164 問セットは `uv run jones-tools fetch-dataset` で一度取得すれば使える（公式 MIT ライセンスのセットを `data/HumanEval.jsonl` に保存し，`--full` でローダが自動検出する）．任意のコピーは `--dataset <HumanEval.jsonl>` で差し替えられる — 採用したセットとその件数は常にログ出力され，サイレントに切り詰められることはない．フルセットは git 管理外で，追跡されるのは subset のみ．

論文のもう一方のデータセット **MathEquations** は著者独自・非公開のため，再現実装では生成する．curate 済み8問を同梱し，`--math-count N --math-seed S` で決定論的にスケールする（8問は安定 prefix，超過分は演算子優先順位テンプレートから生成し，いずれもデータセットの不変条件を保つ）．詳細は [CLI](docs/cli.ja.md#mathequations--生成セットe3e4) を参照．

どのサブコマンドも実行を 1 つの runvault run として `results/jones/{run_slug}/` に記録する．`run` は条件ごとの観測を `events.jsonl` に，run 全体の集約を `metrics.csv` に書く．`sweep` は親 run で，条件ごとに子 run ができる．`reproduce` は論文の値を `reference.csv` に，観測値を `metrics.csv` に，判定表を `artifacts/` に置く．Python ツールは run の外の `results/jones/figures/{run_slug}/` に PNG を生成する．

> **再現の正直性**: `--mock` は «配管» を検証するためのもの — 各バイアスを項目の固定割合で発現する決定論的スクリプトスタブ（論文値には未調整）であり，アンカーを本当に «PASS» することはない．論文の参照値との真の一致はライブモデルでのみ得られる．


## Scratch run

開発中・デバッグ中・動作確認の実行には `--scratch` を付ける．run は `results/_scratch/` に作られ，同期されない．最新の scratch run は `runvault path --scratch` で取得できる．

## ドキュメント

- [実験一覧](docs/studies.ja.md) — 各実験 (E1–E7) の検証内容，変換 `T`，指標 `φ`，データセット，論文アンカー値．
- [ユースケース](docs/usecases.ja.md) — 実践レシピ（オフライン reproduce，単一ライブ実行，sweep，モデル横断比較）．
- [CLI](docs/cli.ja.md) — `jones` サブコマンド（`run` / `sweep` / `reproduce`），全フラグ，`jones-tools` サブコマンド，環境変数，ローカル Ollama 設定．
- [可視化](docs/visualization.ja.md) — Python `jones-tools` と図の読み方．
- [アーキテクチャ](docs/architecture.ja.md) — ワークスペース構成，モジュールマップ，socsim 委譲，サンドボックス，参考文献．

## 依存

- **Rust**: `clap`（CLI），`serde` + `serde_json`（設定），`anyhow`（エラー），および socsim crate — `socsim-llm`（logprob 非依存生成．Ollama→OpenAI ライブフォールバック + プロンプトキャッシュ．mock 用 `ScriptedClient`），`socsim-metrics`（`stats::mean`），`socsim-reproduce`（論文アンカーハーネス），および `runvault`（実行の記録）．socsim のコミットは `Cargo.lock` で固定．機能的正解率の採点はサンドボックス単体テストのため Python インタプリタを呼び出す．ライブ生成にはローカル Ollama が必要．
- **Python**（`uv`）: `matplotlib`，`numpy`，`pandas`．

## ライセンス

本リポジトリのコードは MIT License で公開される．

## 参考文献

- Jones, E., & Steinhardt, J. (2022). Capturing Failures of Large Language Models via Human Cognitive Biases. *Advances in Neural Information Processing Systems (NeurIPS)*, 35. [arXiv:2202.12299](https://arxiv.org/abs/2202.12299).
- Chen, M., et al. (2021). Evaluating Large Language Models Trained on Code. *arXiv* [arXiv:2107.03374](https://arxiv.org/abs/2107.03374)（Codex / HumanEval / 機能的正解率）．
- Jacowitz, K. E., & Kahneman, D. (1995). Measures of Anchoring in Estimation Tasks. *Personality and Social Psychology Bulletin*, 21(11), 1161–1166（E5）．
- Tversky, A., & Kahneman, D. (1981). The Framing of Decisions and the Psychology of Choice. *Science*, 211(4481), 453–458（E6, Asian Disease）．
