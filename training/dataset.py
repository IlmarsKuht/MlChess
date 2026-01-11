"""
Dataset loading and preprocessing for chess position training.

Supports loading from:
- PGN files (with python-chess)
- Pre-processed numpy arrays
- Lichess database format
"""

import os
import numpy as np
import torch
from torch.utils.data import Dataset, DataLoader
from typing import Optional, Tuple, List
import random

try:
    import chess
    import chess.pgn
    HAS_CHESS = True
except ImportError:
    HAS_CHESS = False
    print("Warning: python-chess not installed. PGN loading disabled.")


# Piece to plane mapping (matching Rust features.rs)
PIECE_TO_PLANE = {
    chess.PAWN: 0,
    chess.KNIGHT: 1,
    chess.BISHOP: 2,
    chess.ROOK: 3,
    chess.QUEEN: 4,
    chess.KING: 5,
}


def board_to_features(board: 'chess.Board') -> np.ndarray:
    """
    Convert a chess.Board to a feature array matching Rust encoding.
    
    Returns: (768,) array with 12 planes × 64 squares
    """
    features = np.zeros(768, dtype=np.float32)
    
    for square in chess.SQUARES:
        piece = board.piece_at(square)
        if piece is not None:
            plane = PIECE_TO_PLANE[piece.piece_type]
            if piece.color == chess.BLACK:
                plane += 6
            idx = plane * 64 + square
            features[idx] = 1.0
    
    return features


def board_to_features_relative(board: 'chess.Board') -> np.ndarray:
    """
    Convert board to features from side-to-move perspective.
    
    If black to move, board is flipped so NN always sees position
    from the perspective of the player to move.
    """
    features = np.zeros(768, dtype=np.float32)
    flip = board.turn == chess.BLACK
    
    for square in chess.SQUARES:
        piece = board.piece_at(square)
        if piece is not None:
            # Flip square if black to move
            target_sq = (63 - square) if flip else square
            
            # Determine if piece is friendly
            is_friendly = piece.color == board.turn
            
            plane = PIECE_TO_PLANE[piece.piece_type]
            if not is_friendly:
                plane += 6
            
            idx = plane * 64 + target_sq
            features[idx] = 1.0
    
    return features


def game_result_to_value(result: str, perspective: bool) -> float:
    """
    Convert game result string to value.
    
    Args:
        result: "1-0", "0-1", or "1/2-1/2"
        perspective: True for white, False for black
    
    Returns: Value in [-1, 1]
    """
    if result == "1-0":
        return 1.0 if perspective else -1.0
    elif result == "0-1":
        return -1.0 if perspective else 1.0
    else:
        return 0.0


class ChessPositionDataset(Dataset):
    """Dataset of chess positions with evaluations."""
    
    def __init__(self, features: np.ndarray, values: np.ndarray):
        """
        Args:
            features: (N, 768) array of position features
            values: (N,) array of evaluation values in [-1, 1]
        """
        self.features = torch.from_numpy(features).float()
        self.values = torch.from_numpy(values).float().unsqueeze(1)
    
    def __len__(self):
        return len(self.features)
    
    def __getitem__(self, idx):
        return self.features[idx], self.values[idx]
    
    @classmethod
    def from_pgn(cls, pgn_path: str, max_games: int = 10000, 
                 positions_per_game: int = 10) -> 'ChessPositionDataset':
        """
        Load positions from a PGN file.
        
        Args:
            pgn_path: Path to PGN file
            max_games: Maximum number of games to process
            positions_per_game: Number of random positions to sample per game
        """
        if not HAS_CHESS:
            raise ImportError("python-chess required for PGN loading")
        
        features_list = []
        values_list = []
        
        with open(pgn_path) as f:
            for game_idx in range(max_games):
                game = chess.pgn.read_game(f)
                if game is None:
                    break
                
                result = game.headers.get("Result", "*")
                if result not in ["1-0", "0-1", "1/2-1/2"]:
                    continue
                
                # Collect all positions from game
                board = game.board()
                positions = []
                
                for move in game.mainline_moves():
                    board.push(move)
                    if len(list(board.legal_moves)) > 0:  # Skip terminal positions
                        positions.append((
                            board_to_features_relative(board.copy()),
                            game_result_to_value(result, board.turn)
                        ))
                
                # Sample positions
                if len(positions) > positions_per_game:
                    positions = random.sample(positions, positions_per_game)
                
                for feat, val in positions:
                    features_list.append(feat)
                    values_list.append(val)
                
                if (game_idx + 1) % 1000 == 0:
                    print(f"Processed {game_idx + 1} games, {len(features_list)} positions")
        
        features = np.stack(features_list)
        values = np.array(values_list, dtype=np.float32)
        
        return cls(features, values)
    
    @classmethod
    def from_numpy(cls, features_path: str, values_path: str) -> 'ChessPositionDataset':
        """Load from pre-processed numpy arrays."""
        features = np.load(features_path)
        values = np.load(values_path)
        return cls(features, values)
    
    def save(self, features_path: str, values_path: str):
        """Save to numpy arrays for faster loading."""
        np.save(features_path, self.features.numpy())
        np.save(values_path, self.values.numpy())


def create_dataloaders(
    dataset: ChessPositionDataset,
    batch_size: int = 128,
    val_split: float = 0.1,
    num_workers: int = 4
) -> Tuple[DataLoader, DataLoader]:
    """
    Split dataset and create train/val dataloaders.
    """
    n = len(dataset)
    n_val = int(n * val_split)
    n_train = n - n_val
    
    train_dataset, val_dataset = torch.utils.data.random_split(
        dataset, [n_train, n_val]
    )
    
    train_loader = DataLoader(
        train_dataset,
        batch_size=batch_size,
        shuffle=True,
        num_workers=num_workers,
        pin_memory=True
    )
    
    val_loader = DataLoader(
        val_dataset,
        batch_size=batch_size,
        shuffle=False,
        num_workers=num_workers,
        pin_memory=True
    )
    
    return train_loader, val_loader


if __name__ == "__main__":
    # Test dataset creation
    print("Creating synthetic test dataset...")
    
    features = np.random.randn(1000, 768).astype(np.float32)
    values = np.random.randn(1000).astype(np.float32)
    values = np.tanh(values)  # Normalize to [-1, 1]
    
    dataset = ChessPositionDataset(features, values)
    print(f"Dataset size: {len(dataset)}")
    
    train_loader, val_loader = create_dataloaders(dataset, batch_size=32)
    print(f"Train batches: {len(train_loader)}, Val batches: {len(val_loader)}")
    
    # Test batch
    x, y = next(iter(train_loader))
    print(f"Batch shapes: x={x.shape}, y={y.shape}")
