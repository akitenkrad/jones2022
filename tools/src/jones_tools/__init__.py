"""jones-tools — jones2022 再現の可視化・分析ツール群．

Rust 側 `jones` バイナリが書き出す `results/<run>/{metrics.csv, config.json,
reproduce_summary.csv}` を読み，図と要約を生成する（Phase 1 スライスでは最小実装）．
"""

__all__ = ["__version__"]
__version__ = "0.1.0"
