#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}
impl Color {
    pub fn other(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
    pub fn idx(self) -> usize {
        match self {
            Color::White => 0,
            Color::Black => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: u8, // 0..63
    pub to: u8,   // 0..63
    pub promo: Option<PieceKind>,
    pub is_en_passant: bool,
    pub is_castle: bool,
}

impl Move {
    pub fn new(from: u8, to: u8) -> Self {
        Self {
            from,
            to,
            promo: None,
            is_en_passant: false,
            is_castle: false,
        }
    }
}

// Helpers
pub fn file_of(sq: u8) -> i8 {
    (sq % 8) as i8
}
pub fn rank_of(sq: u8) -> i8 {
    (sq / 8) as i8
}
pub fn sq(file: i8, rank: i8) -> Option<u8> {
    if (0..8).contains(&file) && (0..8).contains(&rank) {
        Some((rank as u8) * 8 + (file as u8))
    } else {
        None
    }
}

pub fn sq_to_coord(sq: u8) -> String {
    let f = (b'a' + (sq % 8)) as char;
    let r = (b'1' + (sq / 8)) as char;
    format!("{f}{r}")
}

pub fn coord_to_sq(c: &str) -> Option<u8> {
    let b = c.as_bytes();
    if b.len() != 2 {
        return None;
    }
    let f = b[0];
    let r = b[1];
    if !(b'a'..=b'h').contains(&f) || !(b'1'..=b'8').contains(&r) {
        return None;
    }
    let file = f - b'a';
    let rank = r - b'1';
    Some(rank * 8 + file)
}
