use arena_core::{AgentVersion, OpeningPosition, Variant};
use arena_runner::starting_board;
use cozy_chess::{Board, util};

use crate::ApiError;

pub(crate) struct MatchConfig<'a> {
    pub(crate) variant: Variant,
    pub(crate) opening: Option<&'a OpeningPosition>,
    pub(crate) opening_seed: Option<u64>,
}

pub(crate) fn ensure_engine_supports_variant(
    version: &AgentVersion,
    variant: Variant,
) -> Result<(), ApiError> {
    if version.capabilities.supports_variant(variant) {
        return Ok(());
    }

    Err(ApiError::BadRequest(format!(
        "engine {} does not support the {} variant",
        version
            .declared_name
            .as_deref()
            .unwrap_or(version.version.as_str()),
        match variant {
            Variant::Standard => "standard",
            Variant::Chess960 => "chess960",
        }
    )))
}

pub(crate) fn resolve_start_board(config: MatchConfig<'_>) -> Result<Board, ApiError> {
    let variant = config.variant;
    let opening = config.opening;
    let seed = config.opening_seed;

    if let Some(opening) = opening {
        return Board::from_fen(&opening.fen, opening.variant.is_chess960())
            .map_err(|err| ApiError::BadRequest(format!("invalid opening FEN: {err}")));
    }

    Ok(starting_board(variant, None, seed))
}

pub(crate) fn resolve_start_state(config: MatchConfig<'_>) -> Result<(Board, String), ApiError> {
    let variant = config.variant;
    let board = resolve_start_board(config)?;
    let start_fen = fen_for_variant(&board, variant);
    Ok((board, start_fen))
}

pub(crate) fn fen_for_variant(board: &Board, variant: Variant) -> String {
    if variant.is_chess960() {
        format!("{board:#}")
    } else {
        board.to_string()
    }
}

pub(crate) fn parse_saved_board(
    variant: Variant,
    fen: &str,
) -> Result<(Board, String), cozy_chess::FenParseError> {
    if !variant.is_chess960() {
        return Board::from_fen(fen, false).map(|board| (board, fen.to_string()));
    }

    match Board::from_fen(fen, true) {
        Ok(board) => Ok((board.clone(), fen_for_variant(&board, variant))),
        Err(original_err) => {
            let Some(repaired) = repair_chess960_castling_fen(fen) else {
                return Err(original_err);
            };
            Board::from_fen(&repaired, true).map(|board| (board, repaired))
        }
    }
}

pub(crate) fn build_replay_frames(
    variant: Variant,
    start_fen: &str,
    moves_uci: &[String],
) -> Result<Vec<String>, ApiError> {
    let (mut board, normalized_start_fen) = parse_saved_board(variant, start_fen)
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("invalid saved start FEN: {err}")))?;
    let mut frames = vec![normalized_start_fen];
    for move_uci in moves_uci {
        let mv = util::parse_uci_move(&board, move_uci).map_err(|err| {
            ApiError::Internal(anyhow::anyhow!("invalid saved move {move_uci}: {err}"))
        })?;
        board.try_play(mv).map_err(|_| {
            ApiError::Internal(anyhow::anyhow!("illegal saved move {move_uci} in replay"))
        })?;
        frames.push(fen_for_variant(&board, variant));
    }
    Ok(frames)
}

fn repair_chess960_castling_fen(fen: &str) -> Option<String> {
    let mut parts: Vec<&str> = fen.split_whitespace().collect();
    if parts.len() != 6 || parts[2] == "-" {
        return None;
    }

    let white = back_rank_castling_files(parts[0], true)?;
    let black = back_rank_castling_files(parts[0], false)?;
    let mut repaired = String::new();
    let mut changed = false;
    for right in parts[2].chars() {
        let replacement = match right {
            'K' => white.short,
            'Q' => white.long,
            'k' => black.short,
            'q' => black.long,
            _ => Some(right),
        }?;
        changed |= replacement != right;
        repaired.push(replacement);
    }
    if !changed {
        return None;
    }

    parts[2] = &repaired;
    Some(parts.join(" "))
}

struct CastlingFiles {
    short: Option<char>,
    long: Option<char>,
}

fn back_rank_castling_files(placement: &str, white: bool) -> Option<CastlingFiles> {
    let rank = placement.split('/').nth(if white { 7 } else { 0 })?;
    let mut king_file = None;
    let mut rook_files = Vec::new();
    let mut file = 0_usize;
    for token in rank.chars() {
        if let Some(empty) = token.to_digit(10) {
            file += empty as usize;
            continue;
        }
        if (white && token == 'K') || (!white && token == 'k') {
            king_file = Some(file);
        } else if (white && token == 'R') || (!white && token == 'r') {
            rook_files.push(file);
        }
        file += 1;
    }
    let king_file = king_file?;
    let short = rook_files
        .iter()
        .copied()
        .find(|rook_file| *rook_file > king_file)
        .and_then(file_char);
    let long = rook_files
        .iter()
        .copied()
        .rev()
        .find(|rook_file| *rook_file < king_file)
        .and_then(file_char);
    Some(CastlingFiles {
        short: short.map(|value| {
            if white {
                value.to_ascii_uppercase()
            } else {
                value
            }
        }),
        long: long.map(|value| {
            if white {
                value.to_ascii_uppercase()
            } else {
                value
            }
        }),
    })
}

fn file_char(file: usize) -> Option<char> {
    (file < 8).then(|| (b'a' + file as u8) as char)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chess960_start_state_uses_shredder_castling_rights() {
        let (_, start_fen) = resolve_start_state(MatchConfig {
            variant: Variant::Chess960,
            opening: None,
            opening_seed: Some(0),
        })
        .unwrap();

        assert_eq!(
            start_fen,
            "bbqnnrkr/pppppppp/8/8/8/8/PPPPPPPP/BBQNNRKR w HFhf - 0 1"
        );
        Board::from_fen(&start_fen, true).unwrap();
    }

    #[test]
    fn replay_repairs_legacy_chess960_castling_rights() {
        let frames = build_replay_frames(
            Variant::Chess960,
            "qnnrkrbb/pppppppp/8/8/8/8/PPPPPPPP/QNNRKRBB w KQkq - 0 1",
            &[],
        )
        .unwrap();

        assert_eq!(
            frames,
            vec!["qnnrkrbb/pppppppp/8/8/8/8/PPPPPPPP/QNNRKRBB w FDfd - 0 1"]
        );
    }
}
