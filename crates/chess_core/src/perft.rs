use crate::{board::Position, movegen::legal_moves_into, types::Move};

/// Pure perft node count.
/// Counts all legal positions from the current one down to `depth`.
pub fn perft(pos: &mut Position, depth: u8) -> u64 {
    if depth == 0 {
        return 1;
    }

    fn inner(pos: &mut Position, depth: u8, layers: &mut [Vec<Move>]) -> u64 {
        if depth == 0 {
            return 1;
        }

        let (buf, rest) = layers
            .split_first_mut()
            .expect("perft requires one buffer per remaining ply");

        buf.clear();
        legal_moves_into(pos, buf);

        let mut nodes = 0u64;
        for mv in buf.iter().copied() {
            let undo = pos.make_move(mv);
            nodes += inner(pos, depth - 1, rest);
            pos.unmake_move(mv, undo);
        }
        nodes
    }

    let mut layers = vec![Vec::with_capacity(64); depth as usize];
    inner(pos, depth, &mut layers[..])
}
