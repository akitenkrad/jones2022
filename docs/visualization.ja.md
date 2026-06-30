[English](visualization.md) | **日本語**

# 可視化（Python `jones-tools`）

ワークスペースルートで `uv sync` し，`uv run jones-tools <subcommand>` で起動する．図はヘッドレス（Agg）バックエンドを用いるため，ディスプレイ無しのサンドボックス／CI でも描画される．CLI のディスパッチ先: `visualize`・`visualize-sweep`・`show-experiment-settings`・`reproduce-paper`．各々，結果ディレクトリを位置引数またはフラグで取る（既定 `results/latest`）．

## `visualize` — 単一 run の図

```bash
uv run jones-tools visualize results/latest
```

run の `metrics.csv` と `config.json` を読み，実験種別を自動判定して，run ディレクトリに図を書き出す:

- **`fig_accuracy.png`**（コード実験 E1–E4 のみ）— 機能的正解率の棒グラフ: `baseline` バー（青）と変種ごとのバー（橙）を並べる．E1 では Table 1 のビュー（framing 行別の正解率低下），E4 では Table 2 のビュー．
- **`fig_indicator.png`**（全実験）— 変種別の失敗指標率 `r`，およびコード実験では感度 `Δ` の第 2 パネル．E5 では変種は高 / 低アンカー更新率と gibberish 率，E6 では save vs die フレームのリスク選択率（Table 9 のビュー），E7 ではファイル削除率．

変種別の正解率 / `r` / `Δ` の表をコンソールに表示する．機能的正解率を持たない実験（E5/E6/E7）は `fig_indicator.png` のみを書き出す．

## `visualize-sweep` — sweep の図

```bash
uv run jones-tools visualize-sweep results/sweep_20260101_120000
```

sweep の `sweep_summary.csv` と `sweep_config.json` を読み，`fig_sweep.png` を書き出す: 掃引パラメータ（x 軸）に対する指標率 `r` を変種ごとに折れ線で描き，コード sweep では `Δ` の第 2 パネルを添える．E7 パッケージ数掃引では Fig 6 のビュー（パッケージ数とともに削除率が上昇），E5 アンカー比率掃引では `p` とともにアンカリング効果が強まる様子を示す．パラメータ値ごとの平均 `r` も表示する．

## `reproduce-paper` — アンカー表からの図

```bash
uv run jones-tools reproduce-paper results/latest [--summary-csv PATH] [--output-dir DIR]
```

`jones reproduce` が書く `reproduce_summary.csv` を読み，`reproduce_paper.png` を描く: アンカー別に論文値と観測値を並べた棒グラフで，バーは `status` で色分け（PASS = 緑，off = 橙，NO_DATA = 灰，«no data» マーカーで観測バー無し）し，各論文値の周りに許容帯を描く．`PASS / off / NO_DATA` の集計を表示する．観測の無いアンカーは論文値と «no data» マーカーのみを描く．

## `show-experiment-settings` — run の確認

```bash
uv run jones-tools show-experiment-settings results/latest
```

run の `config.json`（実験・モデル・seed・データセット・実験別パラメータ）を整形表示し，`results/latest` シンボリックリンクを解決する．

## 図の読み方（定性的）

図は `metrics.csv` / `sweep_summary.csv` / `reproduce_summary.csv` の数値の定性的ビューである．着目点:

| 図 | 着目点 | 実験 |
|---|---|---|
| `fig_accuracy.png` | 変換バーがベースラインバーより低い（実際の正解率低下 `Δ`） | E1–E4 |
| `fig_indicator.png` | バイアス変種で指標率 `r` が高い；E6 は die フレームが save フレームよりリスキー | E1–E7 |
| `fig_sweep.png` | 掃引パラメータに対する `r` の閾値的／単調な傾向 | E7（パッケージ数）/ E5（比率） |
| `reproduce_paper.png` | 観測バーが論文値の許容帯に収まる（PASS，緑） | E1–E7 |

数値は CSV 由来で，図は便宜的ビューである．`--mock` は決定論的スタブのため，真の再現図はライブモデル実行から得られる．
