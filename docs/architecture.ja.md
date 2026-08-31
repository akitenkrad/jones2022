[English](architecture.md) | **日本語**

# アーキテクチャ

## リポジトリ構成

```
replications/jones2022/
├── Cargo.toml                  # Rust workspace (members = ["simulation"])
├── pyproject.toml              # uv workspace (members = ["tools"])
├── simulation/                 # Rust crate `jones-simulation` (bin `jones`)
│   ├── data/
│   │   └── humaneval_sample.jsonl  # バンドル 8 問 HumanEval subset（オフライン用；フルセットは data/HumanEval.jsonl に取得・git 管理外）
│   └── src/
│       ├── main.rs             # clap: run / sweep / reproduce + 結果書き出し
│       ├── lib.rs              # crate root；socsim 委譲表
│       ├── config.rs           # Experiment (E1–E7)，FRAMING_LINES，ANCHOR_LINES，Config
│       ├── datasets.rs         # HumanEval ローダ（subset / フルセット）+ MathEquations（curate 済み8問 + シード生成器）
│       ├── transforms.rs       # 意味保存変換 T（IPF / アンカー関数 / 順序反転 / 矛盾名）
│       ├── indicators.rs       # 失敗指標 φ（逐語コピー / アンカー行 / distractor token）
│       ├── eval.rs             # run_experiment: 変換 → 問い合わせ → sandbox → φ → MetricRow（E1–E4）
│       ├── gpt3.rs             # E5 数値アンカリング + E6 Asian-Disease フレーミング（実行なし）
│       ├── filedelete.rs       # E7 ドライバ（削除ガード経由）
│       ├── sandbox.rs          # 隔離テスト実行 + ファイル削除ガード
│       ├── anchors.rs          # PAPER_ANCHORS: socsim-reproduce 用の論文参照値
│       └── mock.rs             # 決定論的スクリプトオラクル（実験ファミリ別）
├── tools/                      # Python package `jones-tools`（module `jones_tools`，src レイアウト）
│   └── src/jones_tools/
│       ├── cli.py                       # ディスパッチャ: visualize / visualize-sweep / show-experiment-settings / reproduce-paper
│       ├── visualize.py                 # 単一 run の図（実験種別を自動判定）
│       ├── visualize_sweep.py           # sweep の図（param × r / Δ）
│       ├── show_experiment_settings.py  # config.json の整形表示
│       └── reproduce_paper.py           # reproduce_summary.csv からの図
└── docs/                       # バイリンガル（.md + .ja.md）
```

1 つのツリーに 2 プロジェクト: **Cargo workspace**（`simulation`，crate `jones-simulation`，バイナリ `jones`）と **uv workspace**（`tools`，package `jones-tools`）．Rust 側がプローブパイプラインを実行して指標を書き出し，Python 側が可視化を担う．

## socsim 基盤への委譲

jones2022 は集約済み **socsim** ライブラリ上の薄い層である．**プローブ + 指標パイプライン** であり ABM の tick loop ではないため，`socsim-core`/engine/grid/net は **引かない** — 本論文はエージェント・空間・網・時間発展ダイナミクスを持たず，これらのプリミティブは意図的に不使用とする．下回りはすべて委譲し，論文固有の Rust コードは変換・指標・サンドボックスのみである:

| 関心事 | 委譲先 | 使用箇所 |
|---|---|---|
| LLM 生成（logprob 非依存 `complete`） | `socsim-llm` | `eval.rs`，`gpt3.rs`，`filedelete.rs`，`main.rs` |
| 平均・率の集計 | `socsim-metrics::stats` | `eval.rs`，`main.rs` |
| 論文アンカー PASS/off 再現ハーネス | `socsim-reproduce` | `anchors.rs`，`main.rs` |
| run ディレクトリ・命名・`config.json`・指標／イベント／報告値 | `runvault` | `record.rs`，`main.rs` |

socsim crate と `runvault` は `Cargo.lock` で固定した git 依存として取り込む．`socsim-llm` は `live` フィーチャ付きで取り込むため，`ScriptedClient` mock と並んで Ollama→OpenAI ライブフォールバッククライアントが利用できる．

## 二層決定論

socsim コアは決定論的，LLM レイヤは非決定的である．jones2022 はこの分離を明示する:

- **決定論的レイヤ** はパイプラインの配管: データセット構築・変換適用・指標評価・サンドボックス実行・指標集計は，同じ入力に対して再現可能である．
- **LLM レイヤ** は `socsim-llm` に閉じ込める．生成は greedy（`temperature = 0`）で seed を固定し，ライブ経路は Ollama→OpenAI フォールバックを **プロンプトキャッシュ**（プロンプト + モデル名をキー）で包むため，ウォームキャッシュでの再実行は同一の生成を再生する — ノイジーなモデルを再現可能なオラクルへ変える．モデル名・温度・seed・mock フラグは `config.json` に記録する．

## パイプライン

各実験について，`run` はデータセットを構築し，`LlmClient` を注入し，3 つの eval 経路のいずれかを走らせる．いずれも long 形式の `MetricRow`（`experiment, variant, condition, n, functional_accuracy, indicator_rate, delta`）を出力する．

### コード実験（E1–E4）— `eval.rs` + `sandbox.rs`

E1/E2 は **HumanEval**，E3/E4 は **MathEquations** 上で走る．`run_experiment` ドライバは:

1. **ベースラインパス** — 無変換プロンプトでモデルに問い合わせ，`prompt + completion + test` を組み立て，サンドボックスで実行して機能的正解率を採点する．
2. **変換パス** — 意味保存変換 `T`（`transforms.rs`）を適用して再度問い合わせ，正解率を採点し，指標 `φ`（`indicators.rs`）を **completion のみ** に対して評価する（組み立て済みプログラムには評価しない — 注入された行が自明に φ を満たさないようにするため）．
3. **集計** — 条件別の機能的正解率，感度 `Δ = acc(baseline) − acc(transform)`，指標率 `r` を `socsim-metrics::stats::mean` で算出する．

E1 はフレーミング行ごとに変換行を 1 つ生成し，E2 は単一変換上でアンカー行ごとに指標を測り，E3/E4 は 1 つの prompt-prefix 変換と問題ごとの distractor token を φ として用いる．

### 直接回答実験（E5/E6）— `gpt3.rs`

E5（数値アンカリング）と E6（Asian-Disease フレーミング）は **コード実行を伴わない** 選択・数値タスクである．モデルは数値（E5）または文字（E6）で回答し，指標は回答を直接読む（推定がアンカー方向へ動いたか，リスク選択肢を選んだか）．機能的正解率は適用外で `NaN` を記録する．

### ファイル削除（E7）— `filedelete.rs` + 削除ガード

E7 はモデルに「N パッケージをアンインストールする」スクリプトを書かせ，無関係な **保護対象** ファイルを削除するかを測る．生成コードは `sandbox::run_with_deletion_guard` 下で走り，構造的に安全である（下記）．

## サンドボックス — `sandbox.rs`

機能的正解率の採点は，組み立て済みプログラムを **使い捨て tempdir** 内で Python インタプリタへ渡して実行し（子プロセスの作業ディレクトリをその中へ固定，wall-clock タイムアウトで保護）．インタプリタが見つからない場合は失敗ではなく `Unavailable`（正解率は未定）を返すため，Python 不在環境でも指標経路は走る．

**削除ガード**（E7）は誤削除の測定を安全にする: Python プリアンブルが標準削除 API（`os.remove`/`unlink`/`rmdir`/`removedirs`，`shutil.rmtree`，`pathlib.Path.unlink`/`rmdir`）をすべてフックして **対象パスを記録するだけで削除しない** よう差し替え，子はダミーのパッケージファイルと保護ファイルを置いた tempdir 内で走り，escape プリミティブ（`subprocess`，`os.system`，`eval`，…）を含むコードは実行を拒否する．削除は in-process でフックされる — ホストでもサンドボックスでも，何も実際には削除されない．

## 再現ハーネス — `anchors.rs`

`PAPER_ANCHORS` は論文の定量参照値（E1 の逐語コピー率・正解率低下，E2 のアンカー行出現率，E3/E4 の順序反転・named-function 値，E5/E6 の GPT-3 更新率・リスク選択率，E7 のファイル削除率）を保持し，観測ルックアップとともに `socsim_reproduce::build_rows` へ渡す．ハーネスの機構 — PASS/off/NO_DATA 分類と CSV 書き出し — は `socsim-reproduce` にあり，値だけが jones2022 固有である．`reproduce` は直近に完了した `run` の観測（実験単位．`lineage.derived_from` に記録する）を読むか，`--mock` で全実験をオフライン実行して全アンカーの観測値を生成する．論文の値は run の `reference.csv` に，観測値は `metrics.csv` に，同じ指標名で入る．

## 注入とテスト

eval のエントリポイントはすべて `&dyn LlmClient` を取るため，パイプライン全体をテストおよび `--mock` で `socsim_llm::mock::ScriptedClient` により検証できる — ライブモデル不要．mock は各バイアスを項目の固定 ~60% で発現する決定論的スクリプトスタブ（論文値には未調整の任意割合）であり，オフライン実行は中間的な `Δ`/`r` を出す．アンカーとの真の一致はライブモデルでのみ得られる．

## 参考文献

- Jones, E., & Steinhardt, J. (2022). Capturing Failures of Large Language Models via Human Cognitive Biases. *Advances in Neural Information Processing Systems (NeurIPS)*, 35. [arXiv:2202.12299](https://arxiv.org/abs/2202.12299).
- Chen, M., et al. (2021). Evaluating Large Language Models Trained on Code. *arXiv* [arXiv:2107.03374](https://arxiv.org/abs/2107.03374).
