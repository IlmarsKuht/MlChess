"""
Neural Network Architectures for Chess Evaluation

This module defines the neural network models used for position evaluation.
Models are designed to be easily exported to ONNX for Rust inference.
"""

import torch
import torch.nn as nn
import torch.nn.functional as F


class SimpleValueNet(nn.Module):
    """
    Simple feedforward network for position evaluation.
    
    Input: 768 features (12 piece planes × 64 squares)
    Output: Single value in [-1, 1] representing position evaluation
    """
    
    def __init__(self, hidden_sizes=[256, 128, 64]):
        super().__init__()
        
        layers = []
        in_size = 768  # 12 planes × 64 squares
        
        for hidden_size in hidden_sizes:
            layers.append(nn.Linear(in_size, hidden_size))
            layers.append(nn.ReLU())
            layers.append(nn.BatchNorm1d(hidden_size))
            in_size = hidden_size
        
        layers.append(nn.Linear(in_size, 1))
        layers.append(nn.Tanh())  # Output in [-1, 1]
        
        self.net = nn.Sequential(*layers)
    
    def forward(self, x):
        return self.net(x)


class ConvValueNet(nn.Module):
    """
    Convolutional network for position evaluation.
    
    Input: (batch, 12, 8, 8) tensor representing piece planes
    Output: Single value in [-1, 1]
    """
    
    def __init__(self, num_filters=64, num_blocks=4):
        super().__init__()
        
        # Initial convolution
        self.conv_in = nn.Conv2d(12, num_filters, kernel_size=3, padding=1)
        self.bn_in = nn.BatchNorm2d(num_filters)
        
        # Residual blocks
        self.blocks = nn.ModuleList([
            ResidualBlock(num_filters) for _ in range(num_blocks)
        ])
        
        # Value head
        self.conv_val = nn.Conv2d(num_filters, 1, kernel_size=1)
        self.bn_val = nn.BatchNorm2d(1)
        self.fc_val1 = nn.Linear(64, 128)
        self.fc_val2 = nn.Linear(128, 1)
    
    def forward(self, x):
        # Reshape if flat input
        if x.dim() == 2:
            x = x.view(-1, 12, 8, 8)
        
        # Initial conv
        x = F.relu(self.bn_in(self.conv_in(x)))
        
        # Residual blocks
        for block in self.blocks:
            x = block(x)
        
        # Value head
        v = F.relu(self.bn_val(self.conv_val(x)))
        v = v.view(-1, 64)
        v = F.relu(self.fc_val1(v))
        v = torch.tanh(self.fc_val2(v))
        
        return v


class ResidualBlock(nn.Module):
    """Residual block with two convolutions."""
    
    def __init__(self, num_filters):
        super().__init__()
        self.conv1 = nn.Conv2d(num_filters, num_filters, kernel_size=3, padding=1)
        self.bn1 = nn.BatchNorm2d(num_filters)
        self.conv2 = nn.Conv2d(num_filters, num_filters, kernel_size=3, padding=1)
        self.bn2 = nn.BatchNorm2d(num_filters)
    
    def forward(self, x):
        residual = x
        x = F.relu(self.bn1(self.conv1(x)))
        x = self.bn2(self.conv2(x))
        x = F.relu(x + residual)
        return x


class PolicyValueNet(nn.Module):
    """
    Combined policy and value network (AlphaZero-style).
    
    Input: (batch, 12, 8, 8) tensor
    Output: 
        - policy: (batch, 1858) move probabilities
        - value: (batch, 1) position evaluation
    """
    
    # Number of possible moves in UCI format (approximate)
    NUM_MOVES = 1858  # 64*64 + promotions
    
    def __init__(self, num_filters=128, num_blocks=6):
        super().__init__()
        
        # Shared backbone
        self.conv_in = nn.Conv2d(12, num_filters, kernel_size=3, padding=1)
        self.bn_in = nn.BatchNorm2d(num_filters)
        
        self.blocks = nn.ModuleList([
            ResidualBlock(num_filters) for _ in range(num_blocks)
        ])
        
        # Policy head
        self.conv_pol = nn.Conv2d(num_filters, 32, kernel_size=1)
        self.bn_pol = nn.BatchNorm2d(32)
        self.fc_pol = nn.Linear(32 * 64, self.NUM_MOVES)
        
        # Value head
        self.conv_val = nn.Conv2d(num_filters, 1, kernel_size=1)
        self.bn_val = nn.BatchNorm2d(1)
        self.fc_val1 = nn.Linear(64, 128)
        self.fc_val2 = nn.Linear(128, 1)
    
    def forward(self, x):
        if x.dim() == 2:
            x = x.view(-1, 12, 8, 8)
        
        # Shared backbone
        x = F.relu(self.bn_in(self.conv_in(x)))
        for block in self.blocks:
            x = block(x)
        
        # Policy head
        p = F.relu(self.bn_pol(self.conv_pol(x)))
        p = p.view(-1, 32 * 64)
        p = self.fc_pol(p)
        p = F.log_softmax(p, dim=1)
        
        # Value head
        v = F.relu(self.bn_val(self.conv_val(x)))
        v = v.view(-1, 64)
        v = F.relu(self.fc_val1(v))
        v = torch.tanh(self.fc_val2(v))
        
        return p, v


def create_model(architecture="simple", **kwargs):
    """Factory function to create models by name."""
    models = {
        "simple": SimpleValueNet,
        "conv": ConvValueNet,
        "policy_value": PolicyValueNet,
    }
    
    if architecture not in models:
        raise ValueError(f"Unknown architecture: {architecture}. Choose from {list(models.keys())}")
    
    return models[architecture](**kwargs)


if __name__ == "__main__":
    # Test models
    print("Testing SimpleValueNet...")
    model = SimpleValueNet()
    x = torch.randn(4, 768)
    y = model(x)
    print(f"  Input: {x.shape}, Output: {y.shape}")
    
    print("Testing ConvValueNet...")
    model = ConvValueNet()
    x = torch.randn(4, 12, 8, 8)
    y = model(x)
    print(f"  Input: {x.shape}, Output: {y.shape}")
    
    print("Testing PolicyValueNet...")
    model = PolicyValueNet()
    x = torch.randn(4, 12, 8, 8)
    p, v = model(x)
    print(f"  Input: {x.shape}, Policy: {p.shape}, Value: {v.shape}")
    
    print("\nAll models working!")
