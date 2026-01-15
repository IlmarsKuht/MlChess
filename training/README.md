# ML-chess Neural Network Training

Train neural networks for chess position evaluation using Stockfish as the teacher.

## Requirements

1. **Python 3.10+** with dependencies: `pip install -r requirements.txt`
2. **Stockfish** in one of:
   - `training/bin/stockfish/stockfish/` (auto-detected)
   - System PATH
   - `STOCKFISH_PATH` environment variable
3. **Training data** (PGN file) in `training/data/`

## Quick Start

```bash
cd training

# 1. Verify Stockfish is found
python dataset.py

# 2. Train
python train.py --data data/lichess_db_standard_rated_2014-06.pgn --epochs 30 --output ../models/v002/

# 3. Test
cd ..
cargo run -p tournament -- match classical neural:v002 --games 50
```

## Training Options

```
--data              PGN or NPZ file (required)
--output            Model output directory (default: ../models/v002/)
--epochs            Training epochs (default: 30)
--batch-size        Batch size (default: 256)
--lr                Learning rate (default: 0.001)
--num-filters       Conv filters (default: 128, more = stronger)
--num-blocks        Residual blocks (default: 8, more = stronger)
--stockfish-depth   Evaluation depth (default: 12, higher = better but slower)
--max-games         Max games from PGN (default: 50000)
--positions-per-game Positions per game (default: 8)
```

## How It Works

1. Loads chess games from PGN
2. For each position, Stockfish evaluates it at depth 12
3. Positions + evaluations train the neural network
4. Network learns to predict Stockfish's evaluation
5. Exported ONNX model used by Rust engine

First run on a PGN creates a `.npz` cache file for faster subsequent runs.
