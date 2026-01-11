#!/usr/bin/env python3
"""
Main training script for ML-chess neural networks.

Usage:
    python train.py --epochs 10 --batch-size 128 --output ../models/v001/
    python train.py --architecture conv --epochs 20 --data data/positions.npz
"""

import argparse
import os
import sys
from pathlib import Path
from datetime import datetime

import torch
import torch.nn as nn
import torch.optim as optim
from torch.utils.tensorboard import SummaryWriter
from tqdm import tqdm
import toml

from model import create_model, SimpleValueNet
from dataset import ChessPositionDataset, create_dataloaders
import numpy as np


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
    parser.add_argument('--architecture', type=str, default='simple',
                        choices=['simple', 'conv', 'policy_value'],
                        help='Model architecture')
    parser.add_argument('--hidden', type=str, default='256,128,64',
                        help='Hidden layer sizes (comma-separated)')
    parser.add_argument('--epochs', type=int, default=10,
                        help='Number of training epochs')
    parser.add_argument('--batch-size', type=int, default=128,
                        help='Training batch size')
    parser.add_argument('--lr', type=float, default=0.001,
                        help='Learning rate')
    parser.add_argument('--data', type=str, default=None,
                        help='Path to training data (PGN or NPZ)')
    parser.add_argument('--output', type=str, default='../models/v001/',
                        help='Output directory for model and metadata')
    parser.add_argument('--checkpoint-dir', type=str, default='checkpoints/',
                        help='Directory for training checkpoints')
    parser.add_argument('--resume', type=str, default=None,
                        help='Resume from checkpoint')
    parser.add_argument('--synthetic', action='store_true',
                        help='Use synthetic data for testing')
    args = parser.parse_args()
    
    # Setup device
    device = torch.device('cuda' if torch.cuda.is_available() else 'cpu')
    print(f"Using device: {device}")
    
    # Create output directories
    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)
    checkpoint_dir = Path(args.checkpoint_dir)
    checkpoint_dir.mkdir(parents=True, exist_ok=True)
    
    # Create model
    hidden_sizes = [int(x) for x in args.hidden.split(',')]
    if args.architecture == 'simple':
        model = create_model('simple', hidden_sizes=hidden_sizes)
    else:
        model = create_model(args.architecture)
    model = model.to(device)
    print(f"Model: {args.architecture}, Parameters: {sum(p.numel() for p in model.parameters()):,}")
    
    # Load or create dataset
    if args.synthetic or args.data is None:
        print("Using synthetic dataset for testing...")
        features = np.random.randn(10000, 768).astype(np.float32)
        values = np.tanh(np.random.randn(10000)).astype(np.float32)
        dataset = ChessPositionDataset(features, values)
    elif args.data.endswith('.pgn'):
        print(f"Loading PGN: {args.data}")
        dataset = ChessPositionDataset.from_pgn(args.data)
    elif args.data.endswith('.npz'):
        print(f"Loading NPZ: {args.data}")
        data = np.load(args.data)
        dataset = ChessPositionDataset(data['features'], data['values'])
    else:
        raise ValueError(f"Unknown data format: {args.data}")
    
    print(f"Dataset size: {len(dataset)} positions")
    
    # Create dataloaders
    train_loader, val_loader = create_dataloaders(
        dataset, batch_size=args.batch_size
    )
    
    # Setup training
    criterion = nn.MSELoss()
    optimizer = optim.Adam(model.parameters(), lr=args.lr)
    scheduler = optim.lr_scheduler.ReduceLROnPlateau(
        optimizer, mode='min', factor=0.5, patience=3
    )
    
    # Resume from checkpoint if specified
    start_epoch = 0
    if args.resume:
        checkpoint = torch.load(args.resume)
        model.load_state_dict(checkpoint['model_state_dict'])
        optimizer.load_state_dict(checkpoint['optimizer_state_dict'])
        start_epoch = checkpoint['epoch'] + 1
        print(f"Resumed from epoch {start_epoch}")
    
    # TensorBoard
    writer = SummaryWriter(log_dir=f'runs/{datetime.now().strftime("%Y%m%d_%H%M%S")}')
    
    # Training loop
    best_val_loss = float('inf')
    
    for epoch in range(start_epoch, args.epochs):
        print(f"\nEpoch {epoch + 1}/{args.epochs}")
        
        train_loss = train_epoch(model, train_loader, criterion, optimizer, device)
        val_loss = validate(model, val_loader, criterion, device)
        
        scheduler.step(val_loss)
        
        print(f"  Train Loss: {train_loss:.6f}")
        print(f"  Val Loss:   {val_loss:.6f}")
        
        writer.add_scalars('Loss', {
            'train': train_loss,
            'val': val_loss
        }, epoch)
        
        # Save checkpoint
        save_checkpoint(
            model, optimizer, epoch, val_loss,
            checkpoint_dir / f'epoch_{epoch:03d}.pt'
        )
        
        # Save best model
        if val_loss < best_val_loss:
            best_val_loss = val_loss
            save_checkpoint(
                model, optimizer, epoch, val_loss,
                checkpoint_dir / 'best.pt'
            )
            print("  (New best model saved)")
    
    writer.close()
    
    # Export final model
    print(f"\nExporting model to {output_dir}")
    
    # Save PyTorch model
    torch.save(model.state_dict(), output_dir / 'model.pt')
    
    # Export to ONNX
    dummy_input = torch.randn(1, 768).to(device)
    torch.onnx.export(
        model,
        dummy_input,
        output_dir / 'model.onnx',
        input_names=['features'],
        output_names=['value'],
        dynamic_axes={'features': {0: 'batch_size'}, 'value': {0: 'batch_size'}}
    )
    print(f"  Saved model.onnx")
    
    # Update metadata
    metadata = {
        'version': output_dir.name,
        'name': f'{args.architecture} Value Network',
        'description': 'Trained value network for chess position evaluation',
        'architecture': {
            'type': args.architecture,
            'input_features': 768,
            'hidden_layers': hidden_sizes if args.architecture == 'simple' else [],
            'output_type': 'value',
            'activation': 'relu',
        },
        'training': {
            'parent_version': '',
            'created': datetime.now().isoformat(),
            'epochs': args.epochs,
            'batch_size': args.batch_size,
            'learning_rate': args.lr,
            'optimizer': 'adam',
            'dataset': args.data or 'synthetic',
            'dataset_size': len(dataset),
        },
        'metrics': {
            'training_loss': float(train_loss),
            'validation_loss': float(best_val_loss),
            'test_accuracy': 0.0,
            'elo_estimate': 0,
        },
        'notes': {
            'changelog': 'Initial training run',
            'known_issues': '',
            'next_steps': 'Evaluate against classical engine',
        }
    }
    
    with open(output_dir / 'metadata.toml', 'w') as f:
        toml.dump(metadata, f)
    print(f"  Updated metadata.toml")
    
    print("\nTraining complete!")
    print(f"Best validation loss: {best_val_loss:.6f}")
    print(f"Model saved to: {output_dir}")


if __name__ == '__main__':
    main()
