"""Extract inline #[cfg(test)] modules into sibling *_tests.rs files.

Usage (from repo root):
    python scripts/move_inline_tests.py

Behavior:
- For each Rust source listed in TARGET_FILES, the script looks for the first
  occurrence of a `#[cfg(test)] mod tests { ... }` block.
- The block's inner contents are written to a new sibling file named
  `<stem>_tests.rs` (e.g., `bitboard_tests.rs`).
- The original inline module is replaced by a declaration:
      #[cfg(test)]
      mod <stem>_tests;
- Existing formatting outside the tests is preserved. If no test block is
  found, the file is left untouched and logged.

Notes:
- This script is deliberately scoped to known files to avoid accidental edits.
- It assumes the test module braces are balanced and not nested in other items.
"""
from __future__ import annotations

import textwrap
from pathlib import Path
from typing import Optional, Tuple

REPO_ROOT = Path(__file__).resolve().parents[1]

TARGET_FILES = [
    REPO_ROOT / "crates/chess_core/src/bitboard.rs",
    REPO_ROOT / "crates/chess_core/src/zobrist.rs",
    REPO_ROOT / "crates/chess_core/src/attacks.rs",
    REPO_ROOT / "crates/chess_core/src/movegen.rs",
    REPO_ROOT / "crates/chess_core/src/time_control.rs",
    REPO_ROOT / "crates/classical_engine/src/search.rs",
    REPO_ROOT / "crates/ml_engine/src/lib.rs",
    REPO_ROOT / "crates/ml_engine/src/features.rs",
    REPO_ROOT / "crates/tournament/src/elo.rs",
    REPO_ROOT / "crates/tournament/src/match_runner.rs",
]


def find_test_block(text: str) -> Optional[Tuple[int, int, int]]:
    """Locate the #[cfg(test)] mod tests block.

    Returns (cfg_index, brace_start, brace_end) if found; otherwise None.
    brace_start points to '{', brace_end to the matching '}'.
    """
    cfg_idx = text.find("#[cfg(test)]")
    if cfg_idx == -1:
        return None
    mod_idx = text.find("mod tests", cfg_idx)
    if mod_idx == -1:
        return None
    brace_start = text.find("{", mod_idx)
    if brace_start == -1:
        return None

    depth = 0
    for i, ch in enumerate(text[brace_start:], start=brace_start):
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return cfg_idx, brace_start, i
    return None


def move_tests(path: Path) -> bool:
    src = path.read_text()
    loc = find_test_block(src)
    if loc is None:
        print(f"[skip] no test block in {path}")
        return False

    cfg_idx, brace_start, brace_end = loc
    body = src[brace_start + 1 : brace_end]
    body = textwrap.dedent(body.strip("\n")) + "\n"

    module_name = f"{path.stem}_tests"
    new_decl = f"#[cfg(test)]\nmod {module_name};\n"

    prefix = src[:cfg_idx].rstrip()
    suffix = src[brace_end + 1 :].lstrip("\n")
    new_src = prefix + "\n\n" + new_decl + "\n" + suffix

    path.write_text(new_src)
    test_path = path.parent / f"{module_name}.rs"
    test_path.write_text(body)

    print(f"[moved] {path.name} -> {test_path.name}")
    return True


def main() -> None:
    changed = 0
    for file in TARGET_FILES:
        if not file.exists():
            print(f"[missing] {file}")
            continue
        if move_tests(file):
            changed += 1
    print(f"done. changed {changed} file(s)")


if __name__ == "__main__":
    main()
