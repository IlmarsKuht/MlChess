"""
Dataset loading and preprocessing for chess position training.

Requires:
- python-chess for PGN parsing
- Stockfish for position evaluation (REQUIRED - no fallbacks)

Set STOCKFISH_PATH environment variable or place stockfish in training/bin/stockfish/
"""

import os
import numpy as np
import torch
from torch.utils.data import Dataset, DataLoader
from typing import Optional, Tuple
import random
from tqdm import tqdm

import chess
import chess.pgn
import chess.engine


def find_stockfish() -> str:
    """Find Stockfish executable. Raises if not found."""
    if os.environ.get("STOCKFISH_PATH"):
        path = os.environ["STOCKFISH_PATH"]
        if os.path.isfile(path):
            return path
    
    script_dir = os.path.dirname(os.path.abspath(__file__))
    local_paths = [
        os.path.join(script_dir, "bin", "stockfish", "stockfish", "stockfish-windows-x86-64-avx2.exe"),
        os.path.join(script_dir, "bin", "stockfish", "stockfish", "stockfish.exe"),
        os.path.join(script_dir, "bin", "stockfish", "stockfish"),
        os.path.join(script_dir, "bin", "stockfish.exe"),
        os.path.join(script_dir, "bin", "stockfish"),
    ]
    
    for path in local_paths:
        if os.path.isfile(path):
            return path
    
    import shutil
    system_stockfish = shutil.which("stockfish")
    if system_stockfish:
        return system_stockfish
    
    raise FileNotFoundError(
        "Stockfish not found. Please either:\n"
        "  1. Set STOCKFISH_PATH environment variable\n"
        "  2. Place stockfish in training/bin/stockfish/\n"
        "  3. Install stockfish and add to system PATH"
    )


STOCKFISH_PATH = find_stockfish()
print(f"Using Stockfish: {STOCKFISH_PATH}")


PIECE_TO_PLANE = {
    chess.PAWN: 0,
    chess.KNIGHT: 1,
    chess.BISHOP: 2,
    chess.ROOK: 3,
    chess.QUEEN: 4,
    chess.KING: 5,
}


def board_to_features_relative(board: chess.Board) -> np.ndarray:
    """
    Convert board to features from side-to-move perspective.
    Board is flipped for black so NN always sees from mover's perspective.
    """
    features = np.zeros(768, dtype=np.float32)
    flip = board.turn == chess.BLACK
    
    for square in chess.SQUARES:
        piece = board.piece_at(square)
        if piece is not None:
            target_sq = (63 - square) if flip else square
            is_friendly = piece.color == board.turn
            plane = PIECE_TO_PLANE[piece.piece_type]
            if not is_friendly:
                plane += 6
            idx = plane * 64 + target_sq
            features[idx] = 1.0
    
    return features


def stockfish_eval_to_value(score_cp: int, mate_in: Optional[int] = None) -> float:
    """Convert Stockfish eval to [-1, 1]. ±400cp -> ±0.76, mate -> ±1.0."""
    if mate_in is not None:
        return 1.0 if mate_in > 0 else -1.0
    return float(np.tanh(score_cp / 400.0))


def evaluate_position(
    board: chess.Board,
    engine: chess.engine.SimpleEngine,
    depth: int = 8
) -> float:
    """Evaluate position using Stockfish. Returns value in [-1, 1]."""
    info = engine.analyse(board, chess.engine.Limit(depth=depth))
    score = info["score"].relative
    
    if score.is_mate():
        return stockfish_eval_to_value(0, score.mate())
    return stockfish_eval_to_value(score.score())


class ChessPositionDataset(Dataset):
    """Dataset of chess positions with Stockfish evaluations."""
    
    def __init__(self, features: np.ndarray, values: np.ndarray):
        self.features = torch.from_numpy(features).float()
        self.values = torch.from_numpy(values).float().unsqueeze(1)
    
    def __len__(self):
        return len(self.features)
    
    def __getitem__(self, idx):
        return self.features[idx], self.values[idx]
    
    @classmethod
    def from_pgn(
        cls,
        pgn_path: str,
        max_games: int = 10000,
        positions_per_game: int = 10,
        stockfish_depth: int = 8,
        stockfish_threads: int = 4
    ) -> 'ChessPositionDataset':
        """Load positions from PGN with Stockfish evaluations."""
        features_list = []
        values_list = []
        
        engine = chess.engine.SimpleEngine.popen_uci(STOCKFISH_PATH)
        engine.configure({"Threads": stockfish_threads, "Hash": 256})
        print(f"Stockfish: depth={stockfish_depth}, threads={stockfish_threads}")
        
        try:
            with open(pgn_path) as f:
                pbar = tqdm(total=max_games, desc="Games", unit="game")
                game_idx = 0
                while game_idx < max_games:
                    game = chess.pgn.read_game(f)
                    if game is None:
                        break
                    
                    board = game.board()
                    moves = list(game.mainline_moves())
                    positions = []
                    
                    for move in moves:
                        board.push(move)
                        if len(list(board.legal_moves)) > 0:
                            value = evaluate_position(board, engine, depth=stockfish_depth)
                            positions.append((
                                board_to_features_relative(board.copy()),
                                value
                            ))
                    
                    if len(positions) > positions_per_game:
                        positions = random.sample(positions, positions_per_game)
                    
                    for feat, val in positions:
                        features_list.append(feat)
                        values_list.append(val)
                    
                    game_idx += 1
                    pbar.update(1)
                    pbar.set_postfix(positions=len(features_list))
                
                pbar.close()
        finally:
            engine.quit()
        
        if len(features_list) == 0:
            raise ValueError("No positions extracted from PGN file")
        
        features = np.stack(features_list)
        values = np.array(values_list, dtype=np.float32)
        
        print(f"Total: {len(features)} positions from {game_idx} games")
        return cls(features, values)
    
    @classmethod
    def from_npz(cls, path: str) -> 'ChessPositionDataset':
        """Load from pre-processed NPZ file."""
        data = np.load(path)
        return cls(data['features'], data['values'])
    
    def save(self, path: str):
        """Save to NPZ for faster loading."""
        np.savez(
            path,
            features=self.features.numpy(),
            values=self.values.squeeze().numpy()
        )


def create_dataloaders(
    dataset: ChessPositionDataset,
    batch_size: int = 256,
    val_split: float = 0.1,
    num_workers: int = 0
) -> Tuple[DataLoader, DataLoader]:
    """Split dataset and create train/val dataloaders."""
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
    print("Testing Stockfish connection...")
    engine = chess.engine.SimpleEngine.popen_uci(STOCKFISH_PATH)
    board = chess.Board()
    info = engine.analyse(board, chess.engine.Limit(depth=10))
    print(f"Start position eval: {info['score']}")
    engine.quit()
    print("Stockfish working correctly!")
