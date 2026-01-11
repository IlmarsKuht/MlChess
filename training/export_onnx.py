#!/usr/bin/env python3
"""
Export trained PyTorch models to ONNX format for Rust inference.

Usage:
    python export_onnx.py --checkpoint checkpoints/best.pt --output ../models/v001/model.onnx
"""

import argparse
from pathlib import Path
import torch

from model import create_model


def export_to_onnx(checkpoint_path: str, output_path: str, architecture: str = 'simple'):
    """
    Export a PyTorch checkpoint to ONNX format.
    """
    print(f"Loading checkpoint: {checkpoint_path}")
    
    # Load checkpoint
    checkpoint = torch.load(checkpoint_path, map_location='cpu')
    
    # Determine model state
    if 'model_state_dict' in checkpoint:
        state_dict = checkpoint['model_state_dict']
    else:
        state_dict = checkpoint
    
    # Infer architecture from state dict if possible
    # (Simple heuristic based on layer names)
    if any('conv' in k for k in state_dict.keys()):
        if any('fc_pol' in k for k in state_dict.keys()):
            architecture = 'policy_value'
        else:
            architecture = 'conv'
    else:
        architecture = 'simple'
    
    print(f"Detected architecture: {architecture}")
    
    # Create model
    model = create_model(architecture)
    model.load_state_dict(state_dict)
    model.eval()
    
    # Create dummy input
    if architecture == 'simple':
        dummy_input = torch.randn(1, 768)
        input_names = ['features']
        dynamic_axes = {'features': {0: 'batch_size'}, 'value': {0: 'batch_size'}}
    else:
        dummy_input = torch.randn(1, 12, 8, 8)
        input_names = ['board']
        dynamic_axes = {'board': {0: 'batch_size'}}
    
    if architecture == 'policy_value':
        output_names = ['policy', 'value']
        dynamic_axes['policy'] = {0: 'batch_size'}
        dynamic_axes['value'] = {0: 'batch_size'}
    else:
        output_names = ['value']
        dynamic_axes['value'] = {0: 'batch_size'}
    
    # Export
    print(f"Exporting to: {output_path}")
    
    output_dir = Path(output_path).parent
    output_dir.mkdir(parents=True, exist_ok=True)
    
    torch.onnx.export(
        model,
        dummy_input,
        output_path,
        input_names=input_names,
        output_names=output_names,
        dynamic_axes=dynamic_axes,
        opset_version=11,
        do_constant_folding=True,
    )
    
    print("Export successful!")
    
    # Verify export
    try:
        import onnx
        onnx_model = onnx.load(output_path)
        onnx.checker.check_model(onnx_model)
        print("ONNX model verified successfully!")
    except ImportError:
        print("(Skipping ONNX verification - onnx package not installed)")
    except Exception as e:
        print(f"Warning: ONNX verification failed: {e}")
    
    # Print model info
    file_size = Path(output_path).stat().st_size
    print(f"\nModel info:")
    print(f"  Architecture: {architecture}")
    print(f"  File size: {file_size / 1024:.1f} KB")
    print(f"  Input shape: {list(dummy_input.shape)}")


def main():
    parser = argparse.ArgumentParser(description='Export model to ONNX')
    parser.add_argument('--checkpoint', type=str, required=True,
                        help='Path to PyTorch checkpoint')
    parser.add_argument('--output', type=str, required=True,
                        help='Output ONNX file path')
    parser.add_argument('--architecture', type=str, default='auto',
                        choices=['auto', 'simple', 'conv', 'policy_value'],
                        help='Model architecture (auto-detected if not specified)')
    args = parser.parse_args()
    
    export_to_onnx(args.checkpoint, args.output, args.architecture)


if __name__ == '__main__':
    main()
