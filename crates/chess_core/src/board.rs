use crate::attacks::{bishop_attacks, king_attacks, knight_attacks, pawn_attacks, rook_attacks};
use crate::bitboard::Bitboard;
use crate::types::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastlingRights {
    pub wk: bool,
    pub wq: bool,
    pub bk: bool,
    pub bq: bool,
}

/// Piece bitboards indexed by [color][piece_kind].
/// Color: 0 = White, 1 = Black
/// PieceKind: 0 = Pawn, 1 = Knight, 2 = Bishop, 3 = Rook, 4 = Queen, 5 = King
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceBitboards {
    pub by_color: [Bitboard; 2],      // All pieces by color
    pub by_piece: [[Bitboard; 6]; 2], // Pieces by [color][kind]
}

impl Default for PieceBitboards {
    fn default() -> Self {
        Self {
            by_color: [Bitboard::EMPTY; 2],
            by_piece: [[Bitboard::EMPTY; 6]; 2],
        }
    }
}

impl PieceBitboards {
    #[inline(always)]
    pub fn occupied(&self) -> Bitboard {
        self.by_color[0] | self.by_color[1]
    }

    #[inline(always)]
    pub fn empty(&self) -> Bitboard {
        !self.occupied()
    }

    #[inline(always)]
    pub fn set(&mut self, sq: u8, piece: Piece) {
        let color_idx = piece.color.idx();
        let kind_idx = piece.kind.idx();
        self.by_color[color_idx].set(sq);
        self.by_piece[color_idx][kind_idx].set(sq);
    }

    #[inline(always)]
    pub fn clear(&mut self, sq: u8, piece: Piece) {
        let color_idx = piece.color.idx();
        let kind_idx = piece.kind.idx();
        self.by_color[color_idx].clear(sq);
        self.by_piece[color_idx][kind_idx].clear(sq);
    }

    #[inline(always)]
    pub fn pieces(&self, color: Color, kind: PieceKind) -> Bitboard {
        self.by_piece[color.idx()][kind.idx()]
    }

    #[inline(always)]
    pub fn color(&self, color: Color) -> Bitboard {
        self.by_color[color.idx()]
    }
}

#[derive(Clone, Debug)]
pub struct Position {
    pub board: [Option<Piece>; 64],
    pub bitboards: PieceBitboards,
    pub side_to_move: Color,
    pub castling: CastlingRights,
    pub en_passant: Option<u8>, // square behind a pawn that just advanced 2
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
}

#[derive(Clone, Debug)]
pub struct Undo {
    pub captured: Option<Piece>,
    pub castling: CastlingRights,
    pub en_passant: Option<u8>,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
    pub moved_piece: Piece,
    pub rook_move: Option<(u8, u8)>, // (rook_from, rook_to) for castling
    pub ep_captured_sq: Option<u8>,  // square actually captured in en-passant
}

impl Position {
    pub fn startpos() -> Self {
        let mut p = Position {
            board: [None; 64],
            bitboards: PieceBitboards::default(),
            side_to_move: Color::White,
            castling: CastlingRights {
                wk: true,
                wq: true,
                bk: true,
                bq: true,
            },
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 1,
        };

        // Pawns
        for f in 0..8 {
            p.set_piece(
                8 + f,
                Some(Piece {
                    color: Color::White,
                    kind: PieceKind::Pawn,
                }),
            );
            p.set_piece(
                48 + f,
                Some(Piece {
                    color: Color::Black,
                    kind: PieceKind::Pawn,
                }),
            );
        }
        // Back ranks
        let back = [
            PieceKind::Rook,
            PieceKind::Knight,
            PieceKind::Bishop,
            PieceKind::Queen,
            PieceKind::King,
            PieceKind::Bishop,
            PieceKind::Knight,
            PieceKind::Rook,
        ];
        for (f, &kind) in back.iter().enumerate() {
            p.set_piece(
                f as u8,
                Some(Piece {
                    color: Color::White,
                    kind,
                }),
            );
            p.set_piece(
                56 + f as u8,
                Some(Piece {
                    color: Color::Black,
                    kind,
                }),
            );
        }
        p
    }

    pub fn from_fen(fen: &str) -> Self {
        // Forsyth-Edwards Notation parser used by tests and UCI setup.
        let parts: Vec<&str> = fen.split_whitespace().collect();
        assert!(parts.len() >= 4, "Invalid FEN: expected at least 4 fields");

        let board_part = parts[0];
        let stm_part = parts[1];
        let castle_part = parts[2];
        let ep_part = parts[3];
        let halfmove_part = parts.get(4).copied().unwrap_or("0");
        let fullmove_part = parts.get(5).copied().unwrap_or("1");

        let mut board = [None; 64];
        let ranks: Vec<&str> = board_part.split('/').collect();
        assert!(ranks.len() == 8, "Invalid FEN board section");

        for (rank_idx, rank_str) in ranks.iter().enumerate() {
            let mut file: i8 = 0;
            let rank: i8 = 7 - rank_idx as i8; // FEN lists rank 8 .. 1
            for ch in rank_str.chars() {
                if let Some(d) = ch.to_digit(10) {
                    file += d as i8;
                } else {
                    let color = if ch.is_uppercase() {
                        Color::White
                    } else {
                        Color::Black
                    };
                    let kind = match ch.to_ascii_lowercase() {
                        'p' => PieceKind::Pawn,
                        'n' => PieceKind::Knight,
                        'b' => PieceKind::Bishop,
                        'r' => PieceKind::Rook,
                        'q' => PieceKind::Queen,
                        'k' => PieceKind::King,
                        _ => panic!("Invalid piece char in FEN: {}", ch),
                    };
                    let sq = sq(file, rank).expect("Square out of bounds while parsing FEN");
                    board[sq as usize] = Some(Piece { color, kind });
                    file += 1;
                }
                assert!(file <= 8, "Too many files in FEN rank");
            }
            assert!(file == 8, "Not enough files in FEN rank");
        }

        let side_to_move = match stm_part {
            "w" => Color::White,
            "b" => Color::Black,
            _ => panic!("Invalid side to move in FEN: {}", stm_part),
        };

        let mut castling = CastlingRights {
            wk: false,
            wq: false,
            bk: false,
            bq: false,
        };
        if castle_part != "-" {
            for c in castle_part.chars() {
                match c {
                    'K' => castling.wk = true,
                    'Q' => castling.wq = true,
                    'k' => castling.bk = true,
                    'q' => castling.bq = true,
                    _ => panic!("Invalid castling char in FEN: {}", c),
                }
            }
        }

        let en_passant = if ep_part == "-" {
            None
        } else {
            coord_to_sq(ep_part)
        };

        let halfmove_clock: u32 = halfmove_part
            .parse()
            .expect("Invalid halfmove clock in FEN");
        let fullmove_number: u32 = fullmove_part
            .parse()
            .expect("Invalid fullmove number in FEN");

        // Build bitboards from the mailbox board
        let mut bitboards = PieceBitboards::default();
        for sq in 0..64u8 {
            if let Some(piece) = board[sq as usize] {
                bitboards.set(sq, piece);
            }
        }

        Position {
            board,
            bitboards,
            side_to_move,
            castling,
            en_passant,
            halfmove_clock,
            fullmove_number,
        }
    }

    /// Get the king square for a color using bitboards (O(1)).
    #[inline(always)]
    pub fn king_sq(&self, c: Color) -> Option<u8> {
        self.bitboards.pieces(c, PieceKind::King).lsb()
    }

    #[inline(always)]
    pub fn piece_at(&self, sq: u8) -> Option<Piece> {
        self.board[sq as usize]
    }

    /// Set a piece on the board, updating both mailbox and bitboards.
    #[inline(always)]
    pub fn set_piece(&mut self, sq: u8, pc: Option<Piece>) {
        // Clear old piece from bitboards if any
        if let Some(old) = self.board[sq as usize] {
            self.bitboards.clear(sq, old);
        }
        // Set new piece in bitboards if any
        if let Some(new) = pc {
            self.bitboards.set(sq, new);
        }
        self.board[sq as usize] = pc;
    }

    /// Check if a color's king is in check using bitboard attacks.
    #[inline]
    pub fn in_check(&self, c: Color) -> bool {
        let ksq = match self.king_sq(c) {
            Some(s) => s,
            None => return false,
        };
        self.is_square_attacked(ksq, c.other())
    }

    /// Check if a square is attacked by a given color using bitboard lookups.
    #[inline]
    pub fn is_square_attacked(&self, target: u8, by: Color) -> bool {
        let occupied = self.bitboards.occupied();

        // Pawn attacks: check if any enemy pawn attacks this square
        // We look at squares that could attack 'target' - i.e., where a pawn of 'by' would be
        let pawn_attackers = pawn_attacks(target, by != Color::White);
        if !(pawn_attackers & self.bitboards.pieces(by, PieceKind::Pawn)).is_empty() {
            return true;
        }

        // Knight attacks
        if !(knight_attacks(target) & self.bitboards.pieces(by, PieceKind::Knight)).is_empty() {
            return true;
        }

        // King attacks
        if !(king_attacks(target) & self.bitboards.pieces(by, PieceKind::King)).is_empty() {
            return true;
        }

        // Bishop/Queen diagonal attacks
        let bishop_queen = self.bitboards.pieces(by, PieceKind::Bishop)
            | self.bitboards.pieces(by, PieceKind::Queen);
        if !(bishop_attacks(target, occupied) & bishop_queen).is_empty() {
            return true;
        }

        // Rook/Queen orthogonal attacks
        let rook_queen = self.bitboards.pieces(by, PieceKind::Rook)
            | self.bitboards.pieces(by, PieceKind::Queen);
        if !(rook_attacks(target, occupied) & rook_queen).is_empty() {
            return true;
        }

        false
    }

    /// Legacy is_square_attacked using mailbox (for reference, can be removed later)
    #[allow(dead_code)]
    fn is_square_attacked_mailbox(&self, target: u8, by: Color) -> bool {
        // Pawn attacks
        let tf = file_of(target);
        let tr = rank_of(target);
        let pawn_dirs: &[(i8, i8)] = match by {
            Color::White => &[(-1, -1), (1, -1)], // white pawns attack upward in rank, but target attacked from below
            Color::Black => &[(-1, 1), (1, 1)],
        };
        for (df, dr) in pawn_dirs {
            if let Some(s) = sq(tf + df, tr + dr)
                && let Some(pc) = self.piece_at(s)
                && pc.color == by
                && pc.kind == PieceKind::Pawn
            {
                return true;
            }
        }

        // Knight attacks
        let knight = [
            (1, 2),
            (2, 1),
            (-1, 2),
            (-2, 1),
            (1, -2),
            (2, -1),
            (-1, -2),
            (-2, -1),
        ];
        for (df, dr) in knight {
            if let Some(s) = sq(tf + df, tr + dr)
                && let Some(pc) = self.piece_at(s)
                && pc.color == by
                && pc.kind == PieceKind::Knight
            {
                return true;
            }
        }

        // King adjacency
        let king = [
            (1, 1),
            (1, 0),
            (1, -1),
            (0, 1),
            (0, -1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
        ];
        for (df, dr) in king {
            if let Some(s) = sq(tf + df, tr + dr)
                && let Some(pc) = self.piece_at(s)
                && pc.color == by
                && pc.kind == PieceKind::King
            {
                return true;
            }
        }

        // Sliding: bishop/rook/queen
        let diag = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
        let ortho = [(1, 0), (-1, 0), (0, 1), (0, -1)];

        for (df, dr) in diag {
            let mut f = tf + df;
            let mut r = tr + dr;
            while let Some(sq2) = sq(f, r) {
                if let Some(pc) = self.piece_at(sq2) {
                    if pc.color == by
                        && (pc.kind == PieceKind::Bishop || pc.kind == PieceKind::Queen)
                    {
                        return true;
                    }
                    break;
                }
                f += df;
                r += dr;
            }
        }
        for (df, dr) in ortho {
            let mut f = tf + df;
            let mut r = tr + dr;
            while let Some(sq2) = sq(f, r) {
                if let Some(pc) = self.piece_at(sq2) {
                    if pc.color == by && (pc.kind == PieceKind::Rook || pc.kind == PieceKind::Queen)
                    {
                        return true;
                    }
                    break;
                }
                f += df;
                r += dr;
            }
        }

        false
    }

    pub fn make_move(&mut self, mv: Move) -> Undo {
        let from = mv.from;
        let to = mv.to;
        let moved = self.piece_at(from).expect("no piece on from-square");
        let mut captured = self.piece_at(to);
        let prev_castling = self.castling;
        let prev_ep = self.en_passant;
        let prev_hmc = self.halfmove_clock;
        let prev_fmn = self.fullmove_number;

        self.en_passant = None;

        // Halfmove clock reset on capture or pawn move
        let mut reset_hmc = moved.kind == PieceKind::Pawn || captured.is_some();

        // Handle en-passant capture
        let mut ep_captured_sq = None;
        if mv.is_en_passant {
            let dir = match moved.color {
                Color::White => -1,
                Color::Black => 1,
            };
            let cap_rank = rank_of(to) + dir;
            let cap_file = file_of(to);
            if let Some(cs) = sq(cap_file, cap_rank) {
                captured = self.piece_at(cs);
                self.set_piece(cs, None);
                ep_captured_sq = Some(cs);
                reset_hmc = true;
            }
        }

        // Move piece (promotion handled after)
        self.set_piece(from, None);
        self.set_piece(to, Some(moved));

        // Promotion
        if moved.kind == PieceKind::Pawn {
            let r = rank_of(to);
            if (moved.color == Color::White && r == 7) || (moved.color == Color::Black && r == 0) {
                let promo = mv.promo.unwrap_or(PieceKind::Queen);
                self.set_piece(
                    to,
                    Some(Piece {
                        color: moved.color,
                        kind: promo,
                    }),
                );
                reset_hmc = true;
            }
        }

        // Castling rook move
        let mut rook_move = None;
        if mv.is_castle && moved.kind == PieceKind::King {
            // Determine rook squares by destination
            // White: e1->g1 rook h1->f1, e1->c1 rook a1->d1
            // Black: e8->g8 rook h8->f8, e8->c8 rook a8->d8
            let (rf, rt) = match (moved.color, from, to) {
                (Color::White, 4, 6) => (7, 5),
                (Color::White, 4, 2) => (0, 3),
                (Color::Black, 60, 62) => (63, 61),
                (Color::Black, 60, 58) => (56, 59),
                _ => (255, 255),
            };
            if rf != 255 {
                let rook = self.piece_at(rf).unwrap();
                self.set_piece(rf, None);
                self.set_piece(rt, Some(rook));
                rook_move = Some((rf, rt));
            }
            reset_hmc = false; // castling doesn't reset unless capture/pawn; already false
        }

        // Update castling rights if king/rook moved or rook captured
        match moved.color {
            Color::White => {
                if moved.kind == PieceKind::King {
                    self.castling.wk = false;
                    self.castling.wq = false;
                }
                if moved.kind == PieceKind::Rook {
                    if from == 0 {
                        self.castling.wq = false;
                    }
                    if from == 7 {
                        self.castling.wk = false;
                    }
                }
            }
            Color::Black => {
                if moved.kind == PieceKind::King {
                    self.castling.bk = false;
                    self.castling.bq = false;
                }
                if moved.kind == PieceKind::Rook {
                    if from == 56 {
                        self.castling.bq = false;
                    }
                    if from == 63 {
                        self.castling.bk = false;
                    }
                }
            }
        }
        // If rook captured on its home square, remove right
        if let Some(cp) = captured
            && cp.kind == PieceKind::Rook
        {
            match cp.color {
                Color::White => {
                    if to == 0 {
                        self.castling.wq = false;
                    }
                    if to == 7 {
                        self.castling.wk = false;
                    }
                }
                Color::Black => {
                    if to == 56 {
                        self.castling.bq = false;
                    }
                    if to == 63 {
                        self.castling.bk = false;
                    }
                }
            }
        }

        // Double pawn push sets en-passant square
        if moved.kind == PieceKind::Pawn {
            let fr = rank_of(from);
            let tr = rank_of(to);
            if (moved.color == Color::White && fr == 1 && tr == 3)
                || (moved.color == Color::Black && fr == 6 && tr == 4)
            {
                // ep square is the square passed over
                let ep_rank = (fr + tr) / 2;
                let ep_file = file_of(from);
                self.en_passant = sq(ep_file, ep_rank);
            }
        }

        self.halfmove_clock = if reset_hmc {
            0
        } else {
            self.halfmove_clock + 1
        };

        // Switch side
        if self.side_to_move == Color::Black {
            self.fullmove_number += 1;
        }
        self.side_to_move = self.side_to_move.other();

        Undo {
            captured,
            castling: prev_castling,
            en_passant: prev_ep,
            halfmove_clock: prev_hmc,
            fullmove_number: prev_fmn,
            moved_piece: moved,
            rook_move,
            ep_captured_sq,
        }
    }

    pub fn unmake_move(&mut self, mv: Move, undo: Undo) {
        // Restore side
        self.side_to_move = self.side_to_move.other();
        self.castling = undo.castling;
        self.en_passant = undo.en_passant;
        self.halfmove_clock = undo.halfmove_clock;
        self.fullmove_number = undo.fullmove_number;

        let from = mv.from;
        let to = mv.to;

        // Undo castling rook move
        if let Some((rf, rt)) = undo.rook_move {
            let rook = self.piece_at(rt).unwrap();
            self.set_piece(rt, None);
            self.set_piece(rf, Some(rook));
        }

        // Move piece back
        let mut piece_on_to = self.piece_at(to).unwrap();
        // If it was a promotion, revert to pawn
        if undo.moved_piece.kind == PieceKind::Pawn {
            let r = rank_of(to);
            if (undo.moved_piece.color == Color::White && r == 7)
                || (undo.moved_piece.color == Color::Black && r == 0)
            {
                piece_on_to = Piece {
                    color: undo.moved_piece.color,
                    kind: PieceKind::Pawn,
                };
            }
        }

        self.set_piece(to, None);
        self.set_piece(from, Some(piece_on_to));

        // Restore captured piece
        if mv.is_en_passant {
            if let Some(cs) = undo.ep_captured_sq {
                self.set_piece(cs, undo.captured);
            }
        } else {
            self.set_piece(to, undo.captured);
        }
    }

    /// Computes a hash of the position for repetition detection.
    ///
    /// This hash includes:
    /// - Piece positions
    /// - Side to move
    /// - Castling rights
    /// - En passant square
    ///
    /// It does NOT include halfmove clock or fullmove number, as those
    /// don't affect position identity for repetition purposes.
    pub fn position_hash(&self) -> u64 {
        fn mix(mut h: u64, x: u64) -> u64 {
            h ^= x;
            h = h.wrapping_mul(0x100000001b3);
            h
        }

        let mut h = 0xcbf29ce484222325u64;

        // Side to move
        h = mix(
            h,
            match self.side_to_move {
                Color::White => 1,
                Color::Black => 2,
            },
        );

        // Castling rights
        h = mix(h, if self.castling.wk { 3 } else { 5 });
        h = mix(h, if self.castling.wq { 7 } else { 11 });
        h = mix(h, if self.castling.bk { 13 } else { 17 });
        h = mix(h, if self.castling.bq { 19 } else { 23 });

        // En passant square
        if let Some(ep) = self.en_passant {
            h = mix(h, 29 + ep as u64);
        }

        // Board state
        for (i, sq) in self.board.iter().enumerate() {
            let v = if let Some(pc) = sq {
                (i as u64) ^ ((pc.color.idx() as u64) << 6) ^ ((pc.kind as u64) << 3)
            } else {
                i as u64
            };
            h = mix(h, v);
        }

        h
    }

    /// Check if the position is a draw due to insufficient material.
    ///
    /// Returns true for:
    /// - King vs King
    /// - King + Bishop vs King
    /// - King + Knight vs King
    /// - King + Bishop vs King + Bishop (same color bishops)
    pub fn is_insufficient_material(&self) -> bool {
        let mut white_knights = 0;
        let mut white_bishops = 0;
        let mut white_bishop_on_light = false;
        let mut black_knights = 0;
        let mut black_bishops = 0;
        let mut black_bishop_on_light = false;
        let mut has_other_pieces = false;

        for sq in 0..64 {
            if let Some(piece) = self.board[sq] {
                match piece.kind {
                    PieceKind::King => {} // Kings are always present
                    PieceKind::Knight => {
                        if piece.color == Color::White {
                            white_knights += 1;
                        } else {
                            black_knights += 1;
                        }
                    }
                    PieceKind::Bishop => {
                        let is_light_square = (sq / 8 + sq % 8) % 2 == 1;
                        if piece.color == Color::White {
                            white_bishops += 1;
                            if is_light_square {
                                white_bishop_on_light = true;
                            }
                        } else {
                            black_bishops += 1;
                            if is_light_square {
                                black_bishop_on_light = true;
                            }
                        }
                    }
                    PieceKind::Pawn | PieceKind::Rook | PieceKind::Queen => {
                        has_other_pieces = true;
                    }
                }
            }
        }

        // If there are pawns, rooks, or queens, it's not insufficient material
        if has_other_pieces {
            return false;
        }

        let total_knights = white_knights + black_knights;
        let total_bishops = white_bishops + black_bishops;

        // King vs King
        if total_knights == 0 && total_bishops == 0 {
            return true;
        }

        // King + single minor piece vs King
        if total_knights + total_bishops == 1 {
            return true;
        }

        // King + Bishop vs King + Bishop (same color squares)
        if total_knights == 0
            && white_bishops == 1
            && black_bishops == 1
            && white_bishop_on_light == black_bishop_on_light
        {
            return true;
        }

        false
    }

    /// Check for fifty-move rule draw.
    ///
    /// Returns true if 50 moves (100 half-moves) have been made without
    /// a pawn move or capture.
    pub fn is_fifty_move_draw(&self) -> bool {
        self.halfmove_clock >= 100
    }
}
