use arena_core::{OpeningPosition, Variant};
use arena_runner::starting_board;

use crate::ApiError;

pub(crate) fn starting_board_for_human_game(
    variant: Variant,
    opening: Option<&OpeningPosition>,
    seed: Option<u64>,
) -> Result<cozy_chess::Board, ApiError> {
    if let Some(opening) = opening {
        return cozy_chess::Board::from_fen(&opening.fen, opening.variant.is_chess960())
            .map_err(|err| ApiError::BadRequest(format!("invalid opening FEN: {err}")));
    }

    Ok(starting_board(variant, None, seed))
}
