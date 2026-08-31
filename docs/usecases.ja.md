[English](usecases.md) | **日本語**

# ユースケース

Jones & Steinhardt (2022) の本再現実装でできること．各例はプローブパイプライン（またはそのオフライン再現）を実行し，指標を報告する．各実験の概念説明は [実験一覧](studies.ja.md)，全フラグは [CLI](cli.ja.md)，配線は [アーキテクチャ](architecture.ja.md) を参照．

> ライブの `run` / `sweep` 例はローカル **Ollama**（`http://localhost:11434`，コードモデル pull 済み: `ollama pull codellama`）が必要．オフラインスモークは `--mock`．手法は logprob 非依存なので任意のモデルで成立する．

## 1. オフライン reproduce（モデル不要・ダウンロード不要）

変換 → 生成 → サンドボックス → 指標 → reproduce の全経路が決定論的スクリプトクライアントでオフライン実行でき，配管を CI で検証できる:

```bash
cargo run --release -- run --experiment framing --mock --seed 42
cargo run --release -- reproduce --mock
```

`--mock` は各バイアスを項目の固定割合で発現するスクリプトオラクルを注入する．`reproduce --mock` は 7 実験すべてを実行し，論文の E1–E7 アンカーを結果と突き合わせ，`PASS / off / NO_DATA` の集計を表示する．これは配管を検証するもので，mock の «PASS» は偶発的である — 論文値との真の一致はライブモデルでのみ得られる．

## 2. 単一のライブ実験

実コードモデルで 1 実験を実行し，感度と指標を読む:

```bash
cargo run --release -- run --experiment framing --model codellama --seed 42
uv run jones-tools visualize
```

run の `events.jsonl` は `baseline` の正解率と，framing 行ごとの変換後正解率・感度 `Δ`・逐語コピー率 `r` を持つ．`visualize` は `fig_accuracy.png` と `fig_indicator.png` を書き出す．論文の framing 効果は 22.3–30.5pt の正解率低下と最大 81% の逐語コピー率である；頑健な現代モデルは大半の framing 行に抵抗しつつ，`return False` のような型仕様と矛盾する行では効果を示すことがある．

## 3. スコープ付きライブスモーク（数問のみ）

ライブ実行はクエリコストがかかる．先頭 1–2 問にスモークを絞る:

```bash
cargo run --release -- run --experiment framing --model codellama --limit 2
```

`--limit N` は先頭 N 問（E1–E4）のみを残す．採用した HumanEval セットと件数はログ出力され，スコープは明示される．

E1/E2 をフル 164 問の HumanEval で走らせるには，一度取得して `--full` を渡す:

```bash
uv run jones-tools fetch-dataset                         # → data/HumanEval.jsonl（164 問, MIT）
cargo run --release -- run --experiment framing --model codellama --full
```

## 4. 利用可能性と属性置換（E3 / E4）

MathEquations 実験は外部データを必要としない — セットはコード内で生成する:

```bash
cargo run --release -- run --experiment availability --mock
cargo run --release -- run --experiment attribute-substitution --mock
uv run jones-tools visualize
```

各々 `baseline` 行と 1 つの `transform` 行を書き出す．`transform` 行の `Δ` は正解率低下，`r` は出力が distractor になる率（E3 は unary-first 解，E4 は named 演算）である．

セットは curate 済み8問を同梱し，`--math-count N --math-seed S` で論文の ~90問/設定 規模まで決定論的にスケールする（8問は安定 prefix，超過分は演算子優先順位テンプレートから生成）．実際の `n` とシードは `config.json` に記録される:

```bash
cargo run --release -- run --experiment availability --mock --math-count 90 --math-seed 7
```

## 5. GPT-3 再現（E5 / E6）

数値・選択実験はコード実行を必要としない:

```bash
cargo run --release -- run --experiment gpt3-anchoring --mock --anchor-ratio 0.5
cargo run --release -- run --experiment gpt3-framing   --mock --respondents 20
uv run jones-tools visualize
```

E5 はアンカー方向更新率（高 / 低）と gibberish 率を，E6 はフレーム別リスク選択率を報告し，die（損失）フレームが save（利得）フレームよりリスキーになるべきである．

## 6. ファイル削除の閾値（E7 sweep）

パッケージ数を掃引して削除閾値を探す（論文 Fig 6: 2 以下では稀，3 以上で 80% 以上）:

```bash
cargo run --release -- sweep --experiment file-deletion --mock --num-packages-values 1,2,3,4,5,6
uv run jones-tools visualize-sweep
```

生成された「アンインストール」スクリプトは削除ガード下で走るため，何も実際には削除されない．パッケージ数ごとに子 run ができ，その `events.jsonl` が保護ファイル削除率を持つ．`visualize-sweep` がそれらを積んで Fig 6 のビューにする．

## 7. アンカー比率の掃引（E5）

```bash
cargo run --release -- sweep --experiment gpt3-anchoring --mock --anchor-ratio-values 0.1,0.2,0.5,0.8
uv run jones-tools visualize-sweep
```

`visualize-sweep` はアンカー方向更新率（と gibberish 率）をアンカー比率 `p` に対して描く — 論文のアンカリング効果は `p` の増加とともに強まる．

## 8. モデル横断比較

モデル名は `--model` だけなので，同じ実験を複数モデルで実行し，失敗の方向がモデル世代を越えて保存されるかを見られる:

```bash
for m in codellama qwen2.5-coder deepseek-coder starcoder2; do
  cargo run --release -- run --experiment framing --model "$m" --results "results-$m"
done
```

各モデルが自身の結果ツリーを書き出す．`events.jsonl` の `Δ` と `r` を比較する．論文の主張は，強度がモデルサイズや命令チューニングで変動しても，失敗の方向はモデル世代を越えて保存される，というものである．

## 9. run の確認

```bash
uv run jones-tools show-experiment-settings
```

`run.json` が持つ同一性と，`config.json` の `parameters`（実行条件）を整形表示する．
