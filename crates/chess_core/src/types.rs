#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}
impl Color {
    #[inline(always)]
    pub fn other(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
    #[inline(always)]
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

impl PieceKind {
    #[inline(always)]
    pub const fn idx(self) -> usize {
        match self {
            PieceKind::Pawn => 0,
            PieceKind::Knight => 1,
            PieceKind::Bishop => 2,
            PieceKind::Rook => 3,
            PieceKind::Queen => 4,
            PieceKind::King => 5,
        }
    }

    pub const ALL: [PieceKind; 6] = [
        PieceKind::Pawn,
        PieceKind::Knight,
        PieceKind::Bishop,
        PieceKind::Rook,
        PieceKind::Queen,
        PieceKind::King,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

/// Compact move representation packed into 16 bits.
///
/// Layout (16 bits total):
/// - bits 0-5: from square (0-63)
/// - bits 6-11: to square (0-63)
/// - bits 12-13: promotion piece (0=none, 1=knight, 2=bishop, 3=rook, 4=queen)
///   Value 0 means no promotion; 1-4 map to promotion pieces
/// - bit 14: is_en_passant flag
/// - bit 15: is_castle flag
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Move(u16);

impl Move {
    // Bit positions and masks
    const FROM_MASK: u16 = 0x3F;        // bits 0-5
    const TO_SHIFT: u16 = 6;
    const TO_MASK: u16 = 0x3F << 6;     // bits 6-11
    const PROMO_SHIFT: u16 = 12;
    const PROMO_MASK: u16 = 0x07 << 12; // bits 12-14 (3 bits for promo)
    const EP_FLAG: u16 = 1 << 15;       // bit 15
    const CASTLE_FLAG: u16 = 1 << 14;   // bit 14 (swapped with promo for better packing)

    /// Create a simple move from one square to another.
    #[inline(always)]
    pub fn new(from: u8, to: u8) -> Self {
        Self((from as u16) | ((to as u16) << Self::TO_SHIFT))
    }

    /// Get the source square (0-63).
    #[inline(always)]
    pub fn from(self) -> u8 {
        (self.0 & Self::FROM_MASK) as u8
    }

    /// Get the destination square (0-63).
    #[inline(always)]
    pub fn to(self) -> u8 {
        ((self.0 & Self::TO_MASK) >> Self::TO_SHIFT) as u8
    }

    /// Get the promotion piece kind, if any.
    #[inline(always)]
    pub fn promo(self) -> Option<PieceKind> {
        let bits = (self.0 >> Self::PROMO_SHIFT) & 0x07;
        match bits {
            0 => None,
            1 => Some(PieceKind::Knight),
            2 => Some(PieceKind::Bishop),
            3 => Some(PieceKind::Rook),
            4 => Some(PieceKind::Queen),
            _ => None, // Invalid, shouldn't happen
        }
    }

    /// Set the promotion piece kind.
    #[inline(always)]
    pub fn set_promo(&mut self, promo: Option<PieceKind>) {
        // Clear existing promo bits
        self.0 &= !Self::PROMO_MASK;
        // Set new promo bits
        let bits = match promo {
            None => 0,
            Some(PieceKind::Knight) => 1,
            Some(PieceKind::Bishop) => 2,
            Some(PieceKind::Rook) => 3,
            Some(PieceKind::Queen) => 4,
            Some(_) => 0, // Pawn/King can't be promotion targets
        };
        self.0 |= bits << Self::PROMO_SHIFT;
    }

    /// Create a move with promotion.
    #[inline(always)]
    pub fn with_promo(from: u8, to: u8, promo: PieceKind) -> Self {
        let mut mv = Self::new(from, to);
        mv.set_promo(Some(promo));
        mv
    }

    /// Check if this is an en passant capture.
    #[inline(always)]
    pub fn is_en_passant(self) -> bool {
        (self.0 & Self::EP_FLAG) != 0
    }

    /// Set the en passant flag.
    #[inline(always)]
    pub fn set_en_passant(&mut self, value: bool) {
        if value {
            self.0 |= Self::EP_FLAG;
        } else {
            self.0 &= !Self::EP_FLAG;
        }
    }

    /// Check if this is a castling move.
    #[inline(always)]
    pub fn is_castle(self) -> bool {
        (self.0 & Self::CASTLE_FLAG) != 0
    }

    /// Set the castle flag.
    #[inline(always)]
    pub fn set_castle(&mut self, value: bool) {
        if value {
            self.0 |= Self::CASTLE_FLAG;
        } else {
            self.0 &= !Self::CASTLE_FLAG;
        }
    }
}

impl std::fmt::Debug for Move {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Move")
            .field("from", &self.from())
            .field("to", &self.to())
            .field("promo", &self.promo())
            .field("is_en_passant", &self.is_en_passant())
            .field("is_castle", &self.is_castle())
            .finish()
    }
}

// Helpers
#[inline(always)]
pub const fn file_of(sq: u8) -> i8 {
    (sq % 8) as i8
}

#[inline(always)]
pub const fn rank_of(sq: u8) -> i8 {
    (sq / 8) as i8
}

#[inline(always)]
pub const fn sq(file: i8, rank: i8) -> Option<u8> {
    if file >= 0 && file < 8 && rank >= 0 && rank < 8 {
        Some((rank as u8) * 8 + (file as u8))
    } else {
        None
    }
}

#[inline(always)]
pub const fn sq_from_coords(file: u8, rank: u8) -> u8 {
    rank * 8 + file
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
