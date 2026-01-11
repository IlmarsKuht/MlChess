# ML-chess Neural Network Training

This directory contains Python scripts for training neural network chess engines.

## Setup

```bash
# Create virtual environment
python -m venv venv
source venv/bin/activate  # Linux/Mac
venv\Scripts\activate     # Windows

# Install dependencies
pip install -r requirements.txt
```

## Directory Structure

```
training/
├── README.md           # This file
├── requirements.txt    # Python dependencies
├── train.py           # Main training script
├── model.py           # Neural network architecture definitions
├── dataset.py         # Data loading and preprocessing
├── export_onnx.py     # Export trained models to ONNX
├── evaluate.py        # Evaluate model against positions
└── data/              # Training data (not tracked in git)
    ├── .gitkeep
    └── lichess_2023_01.pgn  # Example data file
```

## Training Workflow

### 1. Prepare Data

Download chess games (e.g., from Lichess database):
```bash
# Download and extract games
wget https://database.lichess.org/standard/lichess_db_standard_rated_2023-01.pgn.zst
zstd -d lichess_db_standard_rated_2023-01.pgn.zst
```

### 2. Train Model

```bash
# Train a simple value network
python train.py --epochs 10 --batch-size 128 --output ../models/v001/

# Train with specific architecture
python train.py --architecture cnn --hidden 256,128 --epochs 20
```

### 3. Export to ONNX

```bash
# Export the trained model
python export_onnx.py --checkpoint checkpoints/best.pt --output ../models/v001/model.onnx
```

### 4. Evaluate

```bash
# Test against classical engine
cd .. && cargo run -p tournament -- match classical neural:v001 --games 20
```

## Model Architectures

### SimpleNet (Feedforward)
- Input: 768 features (12 planes × 64 squares)
- Hidden: 256 → 128 → 64
- Output: 1 (value in [-1, 1])

### ConvNet (CNN)
- Input: 12 × 8 × 8 tensor
- Conv layers with residual connections
- Output: 1 (value)

### PolicyValueNet (AlphaZero-style)
- Shared conv backbone
- Policy head: 1858 outputs (all possible moves)
- Value head: 1 output

## Training Tips

1. **Start small**: Train on 100k positions first to verify pipeline
2. **Use validation**: Monitor overfitting with held-out positions
3. **Augment data**: Mirror boards for more training examples
4. **Filter games**: Use higher-rated games for better quality
5. **Track experiments**: Use TensorBoard for loss curves
