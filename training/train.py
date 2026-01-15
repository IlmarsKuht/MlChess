#!/usr/bin/env python3
"""
Training script for ML-chess neural networks.

Requires:
- PGN training data in training/data/
- Stockfish for position evaluation

Usage:
    python train.py --data data/games.pgn --epochs 30 --output ../models/v002/
"""

import argparse
from pathlib import Path
from datetime import datetime

import torch
import torch.nn as nn
import torch.optim as optim
from torch.utils.tensorboard import SummaryWriter
from tqdm import tqdm
import toml
import numpy as np

from model import create_model
from dataset import ChessPositionDataset, create_dataloaders


def train_epoch(model, loader, criterion, optimizer, device):
    """Train for one epoch."""
    model.train()
    total_loss = 0
    
    for features, values in tqdm(loader, desc="Training", leave=False):
        features = features.to(device)
        values = values.to(device)
        
        optimizer.zero_grad()
        outputs = model(features)
        loss = criterion(outputs, values)
        loss.backward()
        optimizer.step()
        
        total_loss += loss.item() * len(features)
    
    return total_loss / len(loader.dataset)


def validate(model, loader, criterion, device):
    """Validate the model."""
    model.eval()
    total_loss = 0
    
    with torch.no_grad():
        for features, values in loader:
            features = features.to(device)
            values = values.to(device)
            
            outputs = model(features)
            loss = criterion(outputs, values)
            total_loss += loss.item() * len(features)
    
    return total_loss / len(loader.dataset)


def save_checkpoint(model, optimizer, epoch, loss, path):
    """Save training checkpoint."""
    torch.save({
        'epoch': epoch,
        'model_state_dict': model.state_dict(),
        'optimizer_state_dict': optimizer.state_dict(),
        'loss': loss,
    }, path)


def main():
    parser = argparse.ArgumentParser(description='Train chess neural network')
    parser.add_argument('--data', type=str, required=True,
                        help='Path to training data (PGN or NPZ)')
    parser.add_argument('--output', type=str, default='../models/v002/',
                        help='Output directory for model')
    parser.add_argument('--epochs', type=int, default=30,
                        help='Number of training epochs')
    parser.add_argument('--batch-size', type=int, default=256,
                        help='Training batch size')
    parser.add_argument('--lr', type=float, default=0.001,
                        help='Learning rate')
    parser.add_argument('--num-filters', type=int, default=128,
                        help='Conv filters (more = stronger but slower)')
    parser.add_argument('--num-blocks', type=int, default=8,
                        help='Residual blocks (more = stronger but slower)')
    parser.add_argument('--stockfish-depth', type=int, default=8,
                        help='Stockfish depth (8=fast, 12=better)')
    parser.add_argument('--stockfish-threads', type=int, default=4,
                        help='Stockfish threads (use your CPU core count)')
    parser.add_argument('--max-games', type=int, default=10000,
                        help='Maximum games to load from PGN')
    parser.add_argument('--positions-per-game', type=int, default=8,
                        help='Positions to sample per game')
    parser.add_argument('--resume', type=str, default=None,
                        help='Resume from checkpoint')
    args = parser.parse_args()
    
    device = torch.device('cuda' if torch.cuda.is_available() else 'cpu')
    print(f"Device: {device}")
    
    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)
    checkpoint_dir = Path('checkpoints')
    checkpoint_dir.mkdir(parents=True, exist_ok=True)
    
    model = create_model('conv', num_filters=args.num_filters, num_blocks=args.num_blocks)
    model = model.to(device)
    param_count = sum(p.numel() for p in model.parameters())
    print(f"Model: ConvValueNet ({args.num_filters} filters, {args.num_blocks} blocks)")
    print(f"Parameters: {param_count:,}")
    
    if args.data.endswith('.pgn'):
        print(f"\nLoading PGN: {args.data}")
        print("Evaluating positions with Stockfish (cached to .npz for future runs)")
        dataset = ChessPositionDataset.from_pgn(
            args.data,
            max_games=args.max_games,
            positions_per_game=args.positions_per_game,
            stockfish_depth=args.stockfish_depth,
            stockfish_threads=args.stockfish_threads
        )
        cache_path = args.data.replace('.pgn', '.npz')
        dataset.save(cache_path)
        print(f"Cached dataset to {cache_path} for faster future runs")
    elif args.data.endswith('.npz'):
        print(f"Loading cached NPZ: {args.data}")
        dataset = ChessPositionDataset.from_npz(args.data)
    else:
        raise ValueError("Data must be .pgn or .npz file")
    
    print(f"Dataset: {len(dataset)} positions")
    
    train_loader, val_loader = create_dataloaders(dataset, batch_size=args.batch_size)
    
    criterion = nn.MSELoss()
    optimizer = optim.Adam(model.parameters(), lr=args.lr)
    scheduler = optim.lr_scheduler.ReduceLROnPlateau(optimizer, mode='min', factor=0.5, patience=3)
    
    start_epoch = 0
    if args.resume:
        checkpoint = torch.load(args.resume)
        model.load_state_dict(checkpoint['model_state_dict'])
        optimizer.load_state_dict(checkpoint['optimizer_state_dict'])
        start_epoch = checkpoint['epoch'] + 1
        print(f"Resumed from epoch {start_epoch}")
    
    writer = SummaryWriter(log_dir=f'runs/{datetime.now().strftime("%Y%m%d_%H%M%S")}')
    
    best_val_loss = float('inf')
    train_loss = 0.0
    
    print(f"\nTraining for {args.epochs} epochs...")
    for epoch in range(start_epoch, args.epochs):
        print(f"\nEpoch {epoch + 1}/{args.epochs}")
        
        train_loss = train_epoch(model, train_loader, criterion, optimizer, device)
        val_loss = validate(model, val_loader, criterion, device)
        
        scheduler.step(val_loss)
        
        print(f"  Train Loss: {train_loss:.6f}")
        print(f"  Val Loss:   {val_loss:.6f}")
        
        writer.add_scalars('Loss', {'train': train_loss, 'val': val_loss}, epoch)
        
        save_checkpoint(model, optimizer, epoch, val_loss, checkpoint_dir / f'epoch_{epoch:03d}.pt')
        
        if val_loss < best_val_loss:
            best_val_loss = val_loss
            save_checkpoint(model, optimizer, epoch, val_loss, checkpoint_dir / 'best.pt')
            print("  (New best)")
    
    writer.close()
    
    print(f"\nExporting to {output_dir}")
    
    torch.save(model.state_dict(), output_dir / 'model.pt')
    
    dummy_input = torch.randn(1, 768).to(device)
    torch.onnx.export(
        model,
        dummy_input,
        output_dir / 'model.onnx',
        input_names=['features'],
        output_names=['value'],
        dynamic_axes={'features': {0: 'batch_size'}, 'value': {0: 'batch_size'}}
    )
    print("  Saved model.onnx")
    
    metadata = {
        'version': output_dir.name,
        'name': 'Conv Value Network',
        'description': 'Chess position evaluation network trained with Stockfish labels',
        'architecture': {
            'type': 'conv',
            'input_features': 768,
            'num_filters': args.num_filters,
            'num_blocks': args.num_blocks,
            'output_type': 'value',
        },
        'training': {
            'created': datetime.now().isoformat(),
            'epochs': args.epochs,
            'batch_size': args.batch_size,
            'learning_rate': args.lr,
            'stockfish_depth': args.stockfish_depth,
            'stockfish_threads': args.stockfish_threads,
            'dataset': args.data,
            'dataset_size': len(dataset),
        },
        'metrics': {
            'training_loss': float(train_loss),
            'validation_loss': float(best_val_loss),
            'elo_estimate': 0,
        },
    }
    
    with open(output_dir / 'metadata.toml', 'w') as f:
        toml.dump(metadata, f)
    print("  Saved metadata.toml")
    
    print(f"\nDone! Best validation loss: {best_val_loss:.6f}")
    print(f"Model saved to: {output_dir}")
    print(f"\nNext: cargo run -p tournament -- match classical neural:{output_dir.name} --games 50")


if __name__ == '__main__':
    main()
