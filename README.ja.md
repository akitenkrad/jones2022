[English](README.md) | **日本語**

# 人間の認知バイアスによる LLM の失敗誘発 — Jones & Steinhardt (2022)

Jones & Steinhardt (2022) "Capturing Failures of Large Language Models via Human Cognitive Biases"（[arXiv:2202.12299](https://arxiv.org/abs/2202.12299)）の再現実装．本論文は**人間の認知バイアス**を参照系として，コード生成 LLM の質的失敗を系統的に誘発する手法を提案する．失敗様式を仮説化し，それを誘発する**意味保存変換**を構成して，(a) 変換が機能的正解率を下げるか（感度 `Δ`），(b) 出力が標的失敗特徴を含むか（指標率 `r`）の 2 点を測る．変換は**ブラックボックスかつ logprob 非依存**（生成テキストのみ）なので任意のコードモデルで成立する．

本リポジトリは集約済み **socsim** ライブラリ上に薄く実装する．LLM 生成は `socsim-llm`，平均・正解率集計は `socsim-metrics`，論文アンカー PASS/off 照合は `socsim-reproduce`，結果 I/O は `socsim-results` に委譲する．jones2022 固有は «意味保存変換・失敗指標 `φ`・コード実行サンドボックス»（Rust `simulation/`）と Python 分析ツール（`tools/`）のみ．**プローブ + 指標パイプライン**であり ABM の tick loop ではないため，`socsim-core`/engine/grid/net は引かない．

## スコープ

論文の分析は実験 **E1–E7** に対応する．全 7 実験を実装している:

| 実験 | 検証内容 | 主指標 | CLI |
|------|---------|--------|-----|
| **E1** フレーミング | 無関係前置関数 (IPF) の前置で本体を逐語コピーするか | 感度 `Δ`・逐語コピー率 | `jones run --experiment framing` |
| **E2** アンカリング | アンカー関数の前置で distractor パターンに引き寄せられるか | アンカー行出現率 | `jones run --experiment anchoring` |
| **E3** 利用可能性 | 演算順序反転で「想起しやすい」素朴解に誘導されるか | `Δ`・unary-first 率 | `jones run --experiment availability` |
| **E4** 属性置換 | 矛盾する関数名が仕様の挙動を上書きするか | `Δ`・named-function 率 | `jones run --experiment attribute-substitution` |
| **E5** GPT-3 アンカリング | 数値推定が上下アンカー `a(1±p)` 方向へ動くか | アンカー方向更新率・gibberish 率 | `jones run --experiment gpt3-anchoring` |
| **E6** GPT-3 フレーミング | Asian-Disease の save/die フレームでリスク選択率が変わるか | フレーム別リスク選択率 | `jones run --experiment gpt3-framing` |
| **E7** ファイル削除 | 「N パッケージ削除」要求で無関係ファイルを誤削除するか | 誤削除率 | `jones run --experiment file-deletion` |

E1/E2 は **HumanEval**（Chen et al. 2021），E3/E4 は小規模 **MathEquations** を用い，生成コードをサンドボックスで単体テスト実行して機能的正解率を測る．E5/E6 は数値・選択タスク（実行なし）．E7 は生成された「アンインストール」コードを **削除ガード**（全削除操作をフックし対象を記録するだけで実削除せず，使い捨て tempdir 内で実行）下で走らせるため，ホスト FS には一切触れない．オフライン実行用に HumanEval の小 subset を同梱（フルセットは `--dataset` / `--full` で差し替え）．

`sweep` はパラメータ（E7 パッケージ数 `--num-packages-values`，E5 アンカー比率 `--anchor-ratio-values`）を掃引し `results/sweep_{ts}/sweep_summary.csv` を書く．`reproduce` は E1–E7 の論文アンカーと観測値を照合する．

## インストールと実行

全コマンドは本ディレクトリから実行する．ライブ生成にはローカルの **Ollama**（コードモデル pull 済み）が必要．`--mock` はバンドル subset とスクリプトクライアントで全経路をオフライン実行する．

```bash
cargo build --release
cargo run --release -- run --experiment framing --mock --seed 42
cargo run --release -- run --experiment anchoring --mock --seed 42
cargo run --release -- run --experiment framing --model codellama --seed 42
cargo run --release -- reproduce --mock

uv sync
uv run jones-tools visualize --results-dir results/latest
uv run jones-tools reproduce-paper --results-dir results/latest
```

各 run は `results/{timestamp}/` に `config.json` と `metrics.csv` を，`reproduce` は加えて `reproduce_summary.csv` と `paper_anchors.csv` を書き出す．

## 参考文献

- Jones, E., & Steinhardt, J. (2022). Capturing Failures of Large Language Models via Human Cognitive Biases. *NeurIPS* 35. [arXiv:2202.12299](https://arxiv.org/abs/2202.12299)
- Chen, M., et al. (2021). Evaluating Large Language Models Trained on Code. [arXiv:2107.03374](https://arxiv.org/abs/2107.03374)
