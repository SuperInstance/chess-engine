//! VDMO chess engine library crate.
//!
//! This crate is the shared core used by:
//! - `vdmo` (UCI/CLI engine binary)
//! - `vdmo-chess-gui` (eframe/egui GUI binary)
//!
//! Design goals (near-term):
//! - Keep the API small and stable.
//! - Make correctness easy to test (perft, invariants).
//! - Keep representation efficient (bitboards) while we iterate.

#![deny(unsafe_code)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::identity_op)]

pub mod core {
    //! Core engine modules.
    //!
    //! We start with a small set of foundational building blocks and grow outward:
    //! `types` -> `mv` -> `board` -> `movegen`/`make` -> `perft` -> `search` -> `uci`.

    pub mod types {
        pub type Bitboard = u64;

        #[derive(Copy, Clone, Eq, PartialEq, Debug)]
        #[repr(u8)]
        pub enum Side {
            Black = 0,
            White = 1,
        }

        impl Side {
            #[inline]
            pub const fn other(self) -> Side {
                match self {
                    Side::Black => Side::White,
                    Side::White => Side::Black,
                }
            }
        }

        #[derive(Copy, Clone, Eq, PartialEq, Debug)]
        #[repr(u8)]
        pub enum Piece {
            Pawn = 0,
            Knight = 1,
            Bishop = 2,
            Rook = 3,
            Queen = 4,
            King = 5,
        }

        pub const PIECE_N: usize = 6;

        #[derive(Copy, Clone, Eq, PartialEq, Debug)]
        #[repr(u8)]
        pub enum MoveType {
            Normal = 0,
            Promotion = 1,
            EnPassant = 2,
            Castle = 3,
        }

        #[inline]
        pub const fn sq(file: u8, rank: u8) -> u8 {
            // 0 = a1, 63 = h8
            rank * 8 + file
        }

        #[inline]
        pub const fn file_of(sq: u8) -> u8 {
            sq & 7
        }

        #[inline]
        pub const fn rank_of(sq: u8) -> u8 {
            sq >> 3
        }
    }

    pub mod mv {
        use super::types::{MoveType, Piece};

        /// Packed move format (16-bit):
        /// - bits 0..=5   : to (0..63)
        /// - bits 6..=11  : from (0..63)
        /// - bits 12..=13 : promotion piece (0..3) => N,B,R,Q (only when MoveType::Promotion)
        /// - bits 14..=15 : MoveType
        #[derive(Copy, Clone, Eq, PartialEq, Debug)]
        pub struct Move(pub u16);

        impl Move {
            pub const NONE: Move = Move(0);

            #[inline]
            pub const fn from(self) -> u8 {
                ((self.0 >> 6) & 0x3f) as u8
            }

            #[inline]
            pub const fn to(self) -> u8 {
                (self.0 & 0x3f) as u8
            }

            #[inline]
            pub const fn move_type(self) -> MoveType {
                match (self.0 >> 14) & 0x3 {
                    0 => MoveType::Normal,
                    1 => MoveType::Promotion,
                    2 => MoveType::EnPassant,
                    _ => MoveType::Castle,
                }
            }

            #[inline]
            pub const fn promo(self) -> Option<Piece> {
                if matches!(self.move_type(), MoveType::Promotion) {
                    let p = ((self.0 >> 12) & 0x3) as u8;
                    Some(match p {
                        0 => Piece::Knight,
                        1 => Piece::Bishop,
                        2 => Piece::Rook,
                        _ => Piece::Queen,
                    })
                } else {
                    None
                }
            }

            #[inline]
            pub const fn new(from: u8, to: u8, mt: MoveType) -> Move {
                Move(((mt as u16) << 14) | ((from as u16) << 6) | (to as u16))
            }

            #[inline]
            pub const fn promotion(from: u8, to: u8, promo: Piece) -> Move {
                // promo: N/B/R/Q only
                let enc = match promo {
                    Piece::Knight => 0,
                    Piece::Bishop => 1,
                    Piece::Rook => 2,
                    Piece::Queen => 3,
                    _ => 3,
                };
                Move(
                    ((MoveType::Promotion as u16) << 14)
                        | ((enc as u16) << 12)
                        | ((from as u16) << 6)
                        | (to as u16),
                )
            }
        }
    }

    pub mod zobrist {
        #![allow(unsafe_code)]
        #![allow(static_mut_refs)]

        use super::types::{Piece, Side, PIECE_N};

        pub struct ZobristKeys {
            pub pieces: [[[u64; 64]; PIECE_N]; 2],
            pub castling: [u64; 16],
            pub ep_file: [u64; 8],
            pub side: u64,
        }

        impl ZobristKeys {
            pub fn new() -> Self {
                // Use a simple PRNG (xorshift64) with fixed seed for reproducibility
                let mut rng = 0x123456789abcdef0u64;
                let mut next = || {
                    rng ^= rng << 13;
                    rng ^= rng >> 7;
                    rng ^= rng << 17;
                    rng
                };

                let mut keys = ZobristKeys {
                    pieces: [[[0; 64]; PIECE_N]; 2],
                    castling: [0; 16],
                    ep_file: [0; 8],
                    side: 0,
                };

                for side in 0..2 {
                    for piece in 0..PIECE_N {
                        for sq in 0..64 {
                            keys.pieces[side][piece][sq] = next();
                        }
                    }
                }

                for i in 0..16 {
                    keys.castling[i] = next();
                }

                for i in 0..8 {
                    keys.ep_file[i] = next();
                }

                keys.side = next();

                keys
            }
        }

        // Global static keys
        static mut ZOBRIST_KEYS: Option<ZobristKeys> = None;

        pub fn init() {
            unsafe {
                if ZOBRIST_KEYS.is_none() {
                    ZOBRIST_KEYS = Some(ZobristKeys::new());
                }
            }
        }

        pub fn keys() -> &'static ZobristKeys {
            unsafe {
                init();
                ZOBRIST_KEYS.as_ref().unwrap()
            }
        }
    }

    pub mod board {
        use super::types::{Bitboard, Piece, Side, PIECE_N};
        use super::zobrist;

        /// State that needs to be saved/restored during make/unmake.
        #[derive(Copy, Clone, Debug)]
        pub struct State {
            pub castling_rights: u8,
            pub ep_square: Option<u8>,
            pub captured_piece: Option<Piece>,
            pub zobrist_key: u64,
        }

        #[derive(Clone, Debug)]
        pub struct Position {
            pub side_to_move: Side,
            /// pieces[side][piece] bitboards
            pub pieces: [[Bitboard; PIECE_N]; 2],
            /// occupancy[side]
            pub occ: [Bitboard; 2],
            /// occupancy both
            pub occ_all: Bitboard,

            /// Castling rights bitflags (KQkq):
            /// - bit 0: White king-side  (K)
            /// - bit 1: White queen-side (Q)
            /// - bit 2: Black king-side  (k)
            /// - bit 3: Black queen-side (q)
            pub castling_rights: u8,

            /// En-passant target square (if any)
            pub ep_square: Option<u8>,

            /// State stack for make/unmake
            pub state_stack: Vec<State>,

            /// Zobrist hash of current position
            pub zobrist_key: u64,
        }

        impl Position {
            pub fn empty() -> Self {
                Self {
                    side_to_move: Side::White,
                    pieces: [[0; PIECE_N]; 2],
                    occ: [0; 2],
                    occ_all: 0,
                    castling_rights: 0,
                    ep_square: None,
                    state_stack: Vec::new(),
                    zobrist_key: 0,
                }
            }

            #[inline]
            pub fn recompute_occ(&mut self) {
                for s in 0..2 {
                    let mut o = 0u64;
                    for p in 0..PIECE_N {
                        o |= self.pieces[s][p];
                    }
                    self.occ[s] = o;
                }
                self.occ_all = self.occ[0] | self.occ[1];
            }

            pub fn startpos() -> Self {
                let mut b = Self::empty();
                b.side_to_move = Side::White;

                // Start position has all castling rights (KQkq).
                b.castling_rights = 0b1111;

                // White pieces
                b.pieces[Side::White as usize][Piece::Pawn as usize] = 0x0000_0000_0000_ff00;
                b.pieces[Side::White as usize][Piece::Rook as usize] = 0x0000_0000_0000_0081;
                b.pieces[Side::White as usize][Piece::Knight as usize] = 0x0000_0000_0000_0042;
                b.pieces[Side::White as usize][Piece::Bishop as usize] = 0x0000_0000_0000_0024;
                b.pieces[Side::White as usize][Piece::Queen as usize] = 0x0000_0000_0000_0008;
                b.pieces[Side::White as usize][Piece::King as usize] = 0x0000_0000_0000_0010;

                // Black pieces
                b.pieces[Side::Black as usize][Piece::Pawn as usize] = 0x00ff_0000_0000_0000;
                b.pieces[Side::Black as usize][Piece::Rook as usize] = 0x8100_0000_0000_0000;
                b.pieces[Side::Black as usize][Piece::Knight as usize] = 0x4200_0000_0000_0000;
                b.pieces[Side::Black as usize][Piece::Bishop as usize] = 0x2400_0000_0000_0000;
                b.pieces[Side::Black as usize][Piece::Queen as usize] = 0x0800_0000_0000_0000;
                b.pieces[Side::Black as usize][Piece::King as usize] = 0x1000_0000_0000_0000;

                b.recompute_occ();
                b.zobrist_key = b.compute_hash();
                b
            }

            fn compute_hash(&self) -> u64 {
                let keys = zobrist::keys();
                let mut hash = 0u64;

                // Hash pieces
                for side in 0..2 {
                    for piece in 0..PIECE_N {
                        let mut bb = self.pieces[side][piece];
                        while bb != 0 {
                            let sq = bb.trailing_zeros() as usize;
                            bb &= bb - 1;
                            hash ^= keys.pieces[side][piece][sq];
                        }
                    }
                }

                // Hash castling rights
                hash ^= keys.castling[self.castling_rights as usize];

                // Hash en-passant
                if let Some(ep_sq) = self.ep_square {
                    let file = (ep_sq & 7) as usize;
                    hash ^= keys.ep_file[file];
                }

                // Hash side to move
                if self.side_to_move == Side::Black {
                    hash ^= keys.side;
                }

                hash
            }

            #[inline]
            fn pop_lsb(bb: &mut u64) -> Option<u8> {
                if *bb == 0 {
                    return None;
                }
                let sq = bb.trailing_zeros() as u8;
                *bb &= *bb - 1;
                Some(sq)
            }

            pub fn is_square_attacked(&self, sq: u8, by: Side) -> bool {
                use super::types::{file_of, rank_of};

                let by_us = by as usize;
                let occ = self.occ_all;
                let target_bb = 1u64 << sq;

                // Pawn attacks
                let pawns = self.pieces[by_us][Piece::Pawn as usize];
                if pawns != 0 {
                    let attacks = match by {
                        Side::White => {
                            ((pawns << 7) & !0x0101_0101_0101_0101)
                                | ((pawns << 9) & !0x8080_8080_8080_8080)
                        }
                        Side::Black => {
                            ((pawns >> 9) & !0x0101_0101_0101_0101)
                                | ((pawns >> 7) & !0x8080_8080_8080_8080)
                        }
                    };
                    if (attacks & target_bb) != 0 {
                        return true;
                    }
                }

                // Knight attacks
                let knights = self.pieces[by_us][Piece::Knight as usize];
                if knights != 0 {
                    let mut bb = knights;
                    while let Some(from) = Self::pop_lsb(&mut bb) {
                        let ff = file_of(from) as i8;
                        let fr = rank_of(from) as i8;
                        const D: [(i8, i8); 8] = [
                            (1, 2),
                            (2, 1),
                            (2, -1),
                            (1, -2),
                            (-1, -2),
                            (-2, -1),
                            (-2, 1),
                            (-1, 2),
                        ];
                        for (df, dr) in D {
                            let nf = ff + df;
                            let nr = fr + dr;
                            if (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                                let to = (nr as u8) * 8 + (nf as u8);
                                if to == sq {
                                    return true;
                                }
                            }
                        }
                    }
                }

                // King attacks
                let king = self.pieces[by_us][Piece::King as usize];
                if king != 0 {
                    let from = king.trailing_zeros() as u8;
                    let ff = file_of(from) as i8;
                    let fr = rank_of(from) as i8;
                    for dr in -1..=1 {
                        for df in -1..=1 {
                            if df == 0 && dr == 0 {
                                continue;
                            }
                            let nf = ff + df;
                            let nr = fr + dr;
                            if (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                                let to = (nr as u8) * 8 + (nf as u8);
                                if to == sq {
                                    return true;
                                }
                            }
                        }
                    }
                }

                // Sliding attacks
                let rooks = self.pieces[by_us][Piece::Rook as usize];
                let bishops = self.pieces[by_us][Piece::Bishop as usize];
                let queens = self.pieces[by_us][Piece::Queen as usize];
                let rq = rooks | queens;
                let bq = bishops | queens;

                let tf = file_of(sq) as i8;
                let tr = rank_of(sq) as i8;

                const ORTHO: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                for (df, dr) in ORTHO {
                    let mut nf = tf + df;
                    let mut nr = tr + dr;
                    while (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                        let nsq = (nr as u8) * 8 + (nf as u8);
                        let bb = 1u64 << nsq;
                        if (occ & bb) != 0 {
                            if (rq & bb) != 0 {
                                return true;
                            }
                            break;
                        }
                        nf += df;
                        nr += dr;
                    }
                }

                const DIAG: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
                for (df, dr) in DIAG {
                    let mut nf = tf + df;
                    let mut nr = tr + dr;
                    while (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                        let nsq = (nr as u8) * 8 + (nf as u8);
                        let bb = 1u64 << nsq;
                        if (occ & bb) != 0 {
                            if (bq & bb) != 0 {
                                return true;
                            }
                            break;
                        }
                        nf += df;
                        nr += dr;
                    }
                }

                false
            }

            fn piece_at(&self, sq: u8) -> Option<(Side, Piece)> {
                let bb = 1u64 << sq;
                for side in [Side::White, Side::Black] {
                    let s = side as usize;
                    for p in 0..PIECE_N {
                        if (self.pieces[s][p] & bb) != 0 {
                            let piece = match p {
                                0 => Piece::Pawn,
                                1 => Piece::Knight,
                                2 => Piece::Bishop,
                                3 => Piece::Rook,
                                4 => Piece::Queen,
                                5 => Piece::King,
                                _ => unreachable!(),
                            };
                            return Some((side, piece));
                        }
                    }
                }
                None
            }

            pub fn make_move(&mut self, mv: super::mv::Move) {
                use super::types::{file_of, rank_of, MoveType};

                // Save state for unmake
                let state = State {
                    castling_rights: self.castling_rights,
                    ep_square: self.ep_square,
                    captured_piece: None, // will be set if capture occurs
                    zobrist_key: self.zobrist_key,
                };
                self.state_stack.push(state);

                let keys = zobrist::keys();

                // Remove old ep hash if present
                if let Some(ep_sq) = self.ep_square {
                    let file = (ep_sq & 7) as usize;
                    self.zobrist_key ^= keys.ep_file[file];
                }

                // Hash old castling rights
                self.zobrist_key ^= keys.castling[self.castling_rights as usize];

                let from = mv.from();
                let to = mv.to();
                let from_bb = 1u64 << from;
                let to_bb = 1u64 << to;

                let us = self.side_to_move as usize;
                let them = self.side_to_move.other() as usize;

                // Identify moving piece
                let moving_piece = self
                    .piece_at(from)
                    .map(|(_, p)| p)
                    .expect("no piece on from square");

                // Clear ep square
                self.ep_square = None;

                match mv.move_type() {
                    MoveType::Castle => {
                        // Castling: move king + rook
                        self.zobrist_key ^= keys.pieces[us][Piece::King as usize][from as usize];
                        self.zobrist_key ^= keys.pieces[us][Piece::King as usize][to as usize];
                        self.pieces[us][Piece::King as usize] &= !from_bb;
                        self.pieces[us][Piece::King as usize] |= to_bb;

                        // Determine rook movement
                        let (rook_from, rook_to) = match (self.side_to_move, to) {
                            (Side::White, 6) => (7u8, 5u8),    // O-O: h1->f1
                            (Side::White, 2) => (0u8, 3u8),    // O-O-O: a1->d1
                            (Side::Black, 62) => (63u8, 61u8), // O-O: h8->f8
                            (Side::Black, 58) => (56u8, 59u8), // O-O-O: a8->d8
                            _ => panic!("invalid castle move"),
                        };
                        self.zobrist_key ^=
                            keys.pieces[us][Piece::Rook as usize][rook_from as usize];
                        self.zobrist_key ^= keys.pieces[us][Piece::Rook as usize][rook_to as usize];
                        self.pieces[us][Piece::Rook as usize] &= !(1u64 << rook_from);
                        self.pieces[us][Piece::Rook as usize] |= 1u64 << rook_to;

                        // Clear castling rights for this side
                        match self.side_to_move {
                            Side::White => self.castling_rights &= !((1 << 0) | (1 << 1)),
                            Side::Black => self.castling_rights &= !((1 << 2) | (1 << 3)),
                        }
                    }
                    MoveType::EnPassant => {
                        // En passant: capture pawn on different square
                        let capture_sq = match self.side_to_move {
                            Side::White => to - 8,
                            Side::Black => to + 8,
                        };
                        self.zobrist_key ^=
                            keys.pieces[them][Piece::Pawn as usize][capture_sq as usize];
                        self.pieces[them][Piece::Pawn as usize] &= !(1u64 << capture_sq);
                        self.state_stack.last_mut().unwrap().captured_piece = Some(Piece::Pawn);

                        // Move pawn
                        self.zobrist_key ^= keys.pieces[us][Piece::Pawn as usize][from as usize];
                        self.zobrist_key ^= keys.pieces[us][Piece::Pawn as usize][to as usize];
                        self.pieces[us][Piece::Pawn as usize] &= !from_bb;
                        self.pieces[us][Piece::Pawn as usize] |= to_bb;
                    }
                    MoveType::Promotion => {
                        // Capture if any
                        if let Some((_, cap_piece)) = self.piece_at(to) {
                            self.zobrist_key ^= keys.pieces[them][cap_piece as usize][to as usize];
                            self.pieces[them][cap_piece as usize] &= !to_bb;
                            self.state_stack.last_mut().unwrap().captured_piece = Some(cap_piece);
                        }

                        // Remove pawn, add promoted piece
                        self.zobrist_key ^= keys.pieces[us][Piece::Pawn as usize][from as usize];
                        self.pieces[us][Piece::Pawn as usize] &= !from_bb;
                        let promo_piece = mv.promo().unwrap_or(Piece::Queen);
                        self.zobrist_key ^= keys.pieces[us][promo_piece as usize][to as usize];
                        self.pieces[us][promo_piece as usize] |= to_bb;
                    }
                    MoveType::Normal => {
                        // Capture if any
                        if let Some((_, cap_piece)) = self.piece_at(to) {
                            self.zobrist_key ^= keys.pieces[them][cap_piece as usize][to as usize];
                            self.pieces[them][cap_piece as usize] &= !to_bb;
                            self.state_stack.last_mut().unwrap().captured_piece = Some(cap_piece);
                        }

                        // Move piece
                        self.zobrist_key ^= keys.pieces[us][moving_piece as usize][from as usize];
                        self.zobrist_key ^= keys.pieces[us][moving_piece as usize][to as usize];
                        self.pieces[us][moving_piece as usize] &= !from_bb;
                        self.pieces[us][moving_piece as usize] |= to_bb;

                        // Double pawn push: set ep square
                        if moving_piece == Piece::Pawn {
                            let rank_diff = (rank_of(to) as i8 - rank_of(from) as i8).abs();
                            if rank_diff == 2 {
                                let ep_sq = match self.side_to_move {
                                    Side::White => from + 8,
                                    Side::Black => from - 8,
                                };
                                self.ep_square = Some(ep_sq);
                                let file = (ep_sq & 7) as usize;
                                self.zobrist_key ^= keys.ep_file[file];
                            }
                        }
                    }
                }

                // Update castling rights based on piece/square
                match moving_piece {
                    Piece::King => match self.side_to_move {
                        Side::White => self.castling_rights &= !((1 << 0) | (1 << 1)),
                        Side::Black => self.castling_rights &= !((1 << 2) | (1 << 3)),
                    },
                    Piece::Rook => {
                        match (self.side_to_move, from) {
                            (Side::White, 0) => self.castling_rights &= !(1 << 1), // a1
                            (Side::White, 7) => self.castling_rights &= !(1 << 0), // h1
                            (Side::Black, 56) => self.castling_rights &= !(1 << 3), // a8
                            (Side::Black, 63) => self.castling_rights &= !(1 << 2), // h8
                            _ => {}
                        }
                    }
                    _ => {}
                }

                // If rook was captured, clear that castling right
                if let Some(cap_piece) = self.state_stack.last().unwrap().captured_piece {
                    if cap_piece == Piece::Rook {
                        match (self.side_to_move.other(), to) {
                            (Side::White, 0) => self.castling_rights &= !(1 << 1),
                            (Side::White, 7) => self.castling_rights &= !(1 << 0),
                            (Side::Black, 56) => self.castling_rights &= !(1 << 3),
                            (Side::Black, 63) => self.castling_rights &= !(1 << 2),
                            _ => {}
                        }
                    }
                }

                // Hash new castling rights
                self.zobrist_key ^= keys.castling[self.castling_rights as usize];

                // Flip side to move
                self.zobrist_key ^= keys.side;
                self.side_to_move = self.side_to_move.other();
                self.recompute_occ();
            }

            pub fn unmake_move(&mut self, mv: super::mv::Move) {
                use super::types::MoveType;

                // Restore state
                let state = self.state_stack.pop().expect("state stack empty");

                // Flip side first (we're undoing the flip from make_move)
                self.side_to_move = self.side_to_move.other();
                let from = mv.from();
                let to = mv.to();
                let from_bb = 1u64 << from;
                let to_bb = 1u64 << to;

                let us = self.side_to_move as usize;
                let them = self.side_to_move.other() as usize;

                match mv.move_type() {
                    MoveType::Castle => {
                        // Undo castle: move king + rook back
                        self.pieces[us][Piece::King as usize] &= !to_bb;
                        self.pieces[us][Piece::King as usize] |= from_bb;

                        let (rook_from, rook_to) = match (self.side_to_move, to) {
                            (Side::White, 6) => (7u8, 5u8),
                            (Side::White, 2) => (0u8, 3u8),
                            (Side::Black, 62) => (63u8, 61u8),
                            (Side::Black, 58) => (56u8, 59u8),
                            _ => panic!("invalid castle move"),
                        };
                        self.pieces[us][Piece::Rook as usize] &= !(1u64 << rook_to);
                        self.pieces[us][Piece::Rook as usize] |= 1u64 << rook_from;
                    }
                    MoveType::EnPassant => {
                        // Undo ep: move pawn back, restore captured pawn
                        self.pieces[us][Piece::Pawn as usize] &= !to_bb;
                        self.pieces[us][Piece::Pawn as usize] |= from_bb;

                        let capture_sq = match self.side_to_move {
                            Side::White => to - 8,
                            Side::Black => to + 8,
                        };
                        self.pieces[them][Piece::Pawn as usize] |= 1u64 << capture_sq;
                    }
                    MoveType::Promotion => {
                        // Undo promotion: remove promoted piece, restore pawn
                        let promo_piece = mv.promo().unwrap_or(Piece::Queen);
                        self.pieces[us][promo_piece as usize] &= !to_bb;
                        self.pieces[us][Piece::Pawn as usize] |= from_bb;

                        // Restore captured piece if any
                        if let Some(cap_piece) = state.captured_piece {
                            self.pieces[them][cap_piece as usize] |= to_bb;
                        }
                    }
                    MoveType::Normal => {
                        // Identify the piece that moved (it's on 'to' square now)
                        let moving_piece = self
                            .piece_at(to)
                            .map(|(_, p)| p)
                            .expect("no piece on to square");

                        // Move piece back
                        self.pieces[us][moving_piece as usize] &= !to_bb;
                        self.pieces[us][moving_piece as usize] |= from_bb;

                        // Restore captured piece if any
                        if let Some(cap_piece) = state.captured_piece {
                            self.pieces[them][cap_piece as usize] |= to_bb;
                        }
                    }
                }

                // Restore state (including zobrist key)
                self.castling_rights = state.castling_rights;
                self.ep_square = state.ep_square;
                self.zobrist_key = state.zobrist_key;

                self.recompute_occ();
            }
        }

        pub fn parse_fen_minimal(fen: &str) -> Result<Position, String> {
            // Minimal parser: piece placement + side to move + castling rights.
            // Ep square / halfmove / fullmove are still TODO.
            let mut parts = fen.split_whitespace();
            let placement = parts.next().ok_or("FEN missing placement")?;
            let stm = parts.next().ok_or("FEN missing side-to-move")?;
            let castling = parts.next().unwrap_or("-"); // tolerate missing fields

            let mut b = Position::empty();
            b.side_to_move = match stm {
                "w" => Side::White,
                "b" => Side::Black,
                _ => return Err("FEN invalid side-to-move".into()),
            };

            // Parse castling rights field (KQkq or "-")
            let mut rights: u8 = 0;
            if castling != "-" {
                for ch in castling.chars() {
                    match ch {
                        'K' => rights |= 1 << 0,
                        'Q' => rights |= 1 << 1,
                        'k' => rights |= 1 << 2,
                        'q' => rights |= 1 << 3,
                        _ => return Err(format!("FEN invalid castling char: {ch}")),
                    }
                }
            }
            b.castling_rights = rights;

            let mut rank: i32 = 7;
            let mut file: i32 = 0;

            for ch in placement.chars() {
                match ch {
                    '/' => {
                        if file != 8 {
                            return Err("FEN rank does not sum to 8".into());
                        }
                        rank -= 1;
                        file = 0;
                        if rank < 0 {
                            return Err("FEN has too many ranks".into());
                        }
                    }
                    '1'..='8' => {
                        file += (ch as u8 - b'0') as i32;
                        if file > 8 {
                            return Err("FEN file overflow".into());
                        }
                    }
                    _ => {
                        let side = if ch.is_ascii_uppercase() {
                            Side::White
                        } else {
                            Side::Black
                        };
                        let piece = match ch.to_ascii_lowercase() {
                            'p' => Piece::Pawn,
                            'n' => Piece::Knight,
                            'b' => Piece::Bishop,
                            'r' => Piece::Rook,
                            'q' => Piece::Queen,
                            'k' => Piece::King,
                            _ => return Err(format!("FEN invalid piece char: {ch}")),
                        };
                        if !(0..8).contains(&file) || !(0..8).contains(&rank) {
                            return Err("FEN piece out of board".into());
                        }
                        let sq = (rank * 8 + file) as u8;
                        b.pieces[side as usize][piece as usize] |= 1u64 << sq;
                        file += 1;
                    }
                }
            }

            if rank != 0 || file != 8 {
                return Err("FEN placement did not resolve to 8x8".into());
            }

            b.recompute_occ();
            b.zobrist_key = b.compute_hash();
            Ok(b)
        }
    }

    pub mod movegen {
        use super::board::Position;
        use super::mv::Move;
        use super::types::{file_of, rank_of, MoveType, Piece, Side};
        use std::sync::OnceLock;

        pub type MoveList = Vec<Move>;

        #[inline]
        fn add_move(list: &mut MoveList, from: u8, to: u8) {
            list.push(Move::new(from, to, MoveType::Normal));
        }

        #[inline]
        fn add_promo(list: &mut MoveList, from: u8, to: u8) {
            list.push(Move::promotion(from, to, Piece::Queen));
            list.push(Move::promotion(from, to, Piece::Rook));
            list.push(Move::promotion(from, to, Piece::Bishop));
            list.push(Move::promotion(from, to, Piece::Knight));
        }

        fn gen_pawn_moves(pos: &Position, list: &mut MoveList) {
            let us = pos.side_to_move as usize;
            let them = pos.side_to_move.other() as usize;
            let pawns = pos.pieces[us][Piece::Pawn as usize];
            let our_occ = pos.occ[us];
            let their_occ = pos.occ[them];
            let occ = pos.occ_all;

            let (forward, start_rank, promo_rank) = match pos.side_to_move {
                Side::White => (8i8, 1u8, 7u8),
                Side::Black => (-8i8, 6u8, 0u8),
            };

            let mut bb = pawns;
            while bb != 0 {
                let from = bb.trailing_zeros() as u8;
                bb &= bb - 1;

                let to = (from as i8 + forward) as u8;
                let to_bb = 1u64 << to;

                // Single push
                if (occ & to_bb) == 0 {
                    if rank_of(to) == promo_rank {
                        add_promo(list, from, to);
                    } else {
                        add_move(list, from, to);

                        // Double push
                        if rank_of(from) == start_rank {
                            let to2 = (from as i8 + 2 * forward) as u8;
                            let to2_bb = 1u64 << to2;
                            if (occ & to2_bb) == 0 {
                                add_move(list, from, to2);
                            }
                        }
                    }
                }

                // Captures
                let f = file_of(from) as i8;
                for df in [-1i8, 1i8] {
                    let nf = f + df;
                    if !(0..=7).contains(&nf) {
                        continue;
                    }
                    let cap_to = (to as i8 + df) as u8;
                    let cap_to_bb = 1u64 << cap_to;

                    if (their_occ & cap_to_bb) != 0 {
                        if rank_of(cap_to) == promo_rank {
                            add_promo(list, from, cap_to);
                        } else {
                            add_move(list, from, cap_to);
                        }
                    } else if Some(cap_to) == pos.ep_square {
                        list.push(Move::new(from, cap_to, MoveType::EnPassant));
                    }
                }
            }
        }

        fn gen_knight_moves(pos: &Position, list: &mut MoveList) {
            let us = pos.side_to_move as usize;
            let knights = pos.pieces[us][Piece::Knight as usize];
            let our_occ = pos.occ[us];

            const DELTAS: [(i8, i8); 8] = [
                (1, 2),
                (2, 1),
                (2, -1),
                (1, -2),
                (-1, -2),
                (-2, -1),
                (-2, 1),
                (-1, 2),
            ];

            let mut bb = knights;
            while bb != 0 {
                let from = bb.trailing_zeros() as u8;
                bb &= bb - 1;

                let ff = file_of(from) as i8;
                let fr = rank_of(from) as i8;

                for (df, dr) in DELTAS {
                    let nf = ff + df;
                    let nr = fr + dr;
                    if !(0..=7).contains(&nf) || !(0..=7).contains(&nr) {
                        continue;
                    }
                    let to = (nr as u8) * 8 + (nf as u8);
                    let to_bb = 1u64 << to;
                    if (our_occ & to_bb) == 0 {
                        add_move(list, from, to);
                    }
                }
            }
        }

        fn gen_sliding_moves(pos: &Position, list: &mut MoveList, piece: Piece) {
            let us = pos.side_to_move as usize;
            let pieces = pos.pieces[us][piece as usize];
            let our_occ = pos.occ[us];
            let occ = pos.occ_all;

            let mut bb = pieces;
            while bb != 0 {
                let from = bb.trailing_zeros() as u8;
                bb &= bb - 1;

                let attacks = match piece {
                    Piece::Bishop => bishop_attacks(from, occ),
                    Piece::Rook => rook_attacks(from, occ),
                    Piece::Queen => rook_attacks(from, occ) | bishop_attacks(from, occ),
                    _ => 0,
                };

                let mut atk = attacks & !our_occ;
                while atk != 0 {
                    let to = atk.trailing_zeros() as u8;
                    atk &= atk - 1;
                    add_move(list, from, to);
                }
            }
        }

        fn gen_king_moves(pos: &Position, list: &mut MoveList) {
            let us = pos.side_to_move as usize;
            let king = pos.pieces[us][Piece::King as usize];
            let our_occ = pos.occ[us];

            if king == 0 {
                return;
            }

            let from = king.trailing_zeros() as u8;
            let ff = file_of(from) as i8;
            let fr = rank_of(from) as i8;

            for dr in -1..=1 {
                for df in -1..=1 {
                    if df == 0 && dr == 0 {
                        continue;
                    }
                    let nf = ff + df;
                    let nr = fr + dr;
                    if !(0..=7).contains(&nf) || !(0..=7).contains(&nr) {
                        continue;
                    }
                    let to = (nr as u8) * 8 + (nf as u8);
                    let to_bb = 1u64 << to;
                    if (our_occ & to_bb) == 0 {
                        add_move(list, from, to);
                    }
                }
            }
        }

        fn gen_castling_moves(pos: &Position, list: &mut MoveList) {
            let us = pos.side_to_move as usize;
            let occ = pos.occ_all;

            match pos.side_to_move {
                Side::White => {
                    // King-side
                    if (pos.castling_rights & (1 << 0)) != 0 {
                        let f1 = 5u8;
                        let g1 = 6u8;
                        if (occ & ((1u64 << f1) | (1u64 << g1))) == 0 {
                            list.push(Move::new(4, g1, MoveType::Castle));
                        }
                    }
                    // Queen-side
                    if (pos.castling_rights & (1 << 1)) != 0 {
                        let b1 = 1u8;
                        let c1 = 2u8;
                        let d1 = 3u8;
                        if (occ & ((1u64 << b1) | (1u64 << c1) | (1u64 << d1))) == 0 {
                            list.push(Move::new(4, c1, MoveType::Castle));
                        }
                    }
                }
                Side::Black => {
                    // King-side
                    if (pos.castling_rights & (1 << 2)) != 0 {
                        let f8 = 61u8;
                        let g8 = 62u8;
                        if (occ & ((1u64 << f8) | (1u64 << g8))) == 0 {
                            list.push(Move::new(60, g8, MoveType::Castle));
                        }
                    }
                    // Queen-side
                    if (pos.castling_rights & (1 << 3)) != 0 {
                        let b8 = 57u8;
                        let c8 = 58u8;
                        let d8 = 59u8;
                        if (occ & ((1u64 << b8) | (1u64 << c8) | (1u64 << d8))) == 0 {
                            list.push(Move::new(60, c8, MoveType::Castle));
                        }
                    }
                }
            }
        }

        pub fn generate_moves(pos: &Position) -> MoveList {
            let mut list = MoveList::new();

            gen_pawn_moves(pos, &mut list);
            gen_knight_moves(pos, &mut list);

            gen_sliding_moves(pos, &mut list, Piece::Bishop);
            gen_sliding_moves(pos, &mut list, Piece::Rook);
            gen_sliding_moves(pos, &mut list, Piece::Queen);

            gen_king_moves(pos, &mut list);
            gen_castling_moves(pos, &mut list);

            list
        }

        #[derive(Clone)]
        struct MagicTables {
            rook_masks: [u64; 64],
            bishop_masks: [u64; 64],
            rook_magics: [u64; 64],
            bishop_magics: [u64; 64],
            rook_shifts: [u8; 64],
            bishop_shifts: [u8; 64],
            rook_offsets: [usize; 64],
            bishop_offsets: [usize; 64],
            rook_attacks: Vec<u64>,
            bishop_attacks: Vec<u64>,
        }

        static MAGIC_TABLES: OnceLock<MagicTables> = OnceLock::new();

        #[inline]
        fn rook_attacks(sq: u8, occ: u64) -> u64 {
            let tables = MAGIC_TABLES.get_or_init(MagicTables::new);
            let mask = tables.rook_masks[sq as usize];
            let idx = ((occ & mask).wrapping_mul(tables.rook_magics[sq as usize])
                >> tables.rook_shifts[sq as usize]) as usize;
            tables.rook_attacks[tables.rook_offsets[sq as usize] + idx]
        }

        #[inline]
        fn bishop_attacks(sq: u8, occ: u64) -> u64 {
            let tables = MAGIC_TABLES.get_or_init(MagicTables::new);
            let mask = tables.bishop_masks[sq as usize];
            let idx = ((occ & mask).wrapping_mul(tables.bishop_magics[sq as usize])
                >> tables.bishop_shifts[sq as usize]) as usize;
            tables.bishop_attacks[tables.bishop_offsets[sq as usize] + idx]
        }

        impl MagicTables {
            fn new() -> Self {
                let rook_magics: [u64; 64] = [
                    0xA8002C000108020, 0x6C00049B0002001, 0x100200010090040, 0x2480041000800801,
                    0x280028004000800, 0x900410008040022, 0x280020001001080, 0x2880002041000080,
                    0xA000800080400034, 0x4808020004000, 0x2290802004801000, 0x411000D00100020,
                    0x402800800040080, 0xB000401004208, 0x2409000100040200, 0x1002100004082,
                    0x22878001E24000, 0x1090810021004010, 0x801030040200012, 0x500808008001000,
                    0xA08018014000880, 0x8000808004000200, 0x201008080010200, 0x801020000441091,
                    0x800080204005, 0x1040200040100048, 0x120200402082, 0xD14880480100080,
                    0x12040280080080, 0x100040080020080, 0x9020010080800200, 0x813241200148449,
                    0x491604001800080, 0x100401000402001, 0x4820010021001040, 0x400402202000812,
                    0x209009005000802, 0x810800601800400, 0x4301083214000150, 0x204026458E001401,
                    0x40204000808000, 0x8001008040010020, 0x8410820820420010, 0x1003001000090020,
                    0x804040008008080, 0x12000810020004, 0x1000100200040208, 0x430000A044020001,
                    0x280009023410300, 0xE0100040002240, 0x200100401700, 0x2244100408008080,
                    0x8000400801980, 0x2000810040200, 0x8010100228810400, 0x2000009044210200,
                    0x4080008040102101, 0x40002080411D01, 0x2005524060000901, 0x502001008400422,
                    0x489A000810200402, 0x1004400080A13, 0x4000011008020084, 0x26002114058042,
                ];
                let bishop_magics: [u64; 64] = [
                    0x89A1121896040240, 0x2004844802002010, 0x2068080051921000, 0x62880A0220200808,
                    0x4042004000000, 0x100822020200011, 0xC00444222012000A, 0x28808801216001,
                    0x400492088408100, 0x201C401040C0084, 0x840800910A0010, 0x82080240060,
                    0x2000840504006000, 0x30010C4108405004, 0x1008005410080802, 0x8144042209100900,
                    0x208081020014400, 0x4800201208CA00, 0xF18140408012008, 0x1004002802102001,
                    0x841000820080811, 0x40200200A42008, 0x800054042000, 0x88010400410C9000,
                    0x520040470104290, 0x1004040051500081, 0x2002081833080021, 0x400C00C010142,
                    0x941408200C002000, 0x658810000806011, 0x188071040440A00, 0x4800404002011C00,
                    0x104442040404200, 0x511080202091021, 0x4022401120400, 0x80C0040400080120,
                    0x8040010040820802, 0x480810700020090, 0x102008E00040242, 0x809005202050100,
                    0x8002024220104080, 0x431008804142000, 0x19001802081400, 0x200014208040080,
                    0x3308082008200100, 0x41010500040C020, 0x4012020C04210308, 0x208220A202004080,
                    0x111040120082000, 0x6803040141280A00, 0x2101004202410000, 0x8200000041108022,
                    0x21082088000, 0x2410204010040, 0x40100400809000, 0x822088220820214,
                    0x40808090012004, 0x910224040218C9, 0x402814422015008, 0x90014004842410,
                    0x1000042304105, 0x10008830412A00, 0x2520081090008908, 0x40102000A0A60140,
                ];

                let mut rook_masks = [0u64; 64];
                let mut bishop_masks = [0u64; 64];
                let mut rook_shifts = [0u8; 64];
                let mut bishop_shifts = [0u8; 64];
                let mut rook_offsets = [0usize; 64];
                let mut bishop_offsets = [0usize; 64];

                let mut rook_total = 0usize;
                let mut bishop_total = 0usize;

                for sq in 0..64 {
                    rook_masks[sq] = rook_mask(sq as u8);
                    bishop_masks[sq] = bishop_mask(sq as u8);

                    let rbits = rook_masks[sq].count_ones() as u8;
                    let bbits = bishop_masks[sq].count_ones() as u8;
                    rook_shifts[sq] = 64 - rbits;
                    bishop_shifts[sq] = 64 - bbits;

                    rook_offsets[sq] = rook_total;
                    bishop_offsets[sq] = bishop_total;

                    rook_total += 1usize << rbits;
                    bishop_total += 1usize << bbits;
                }

                let mut rook_attacks = vec![0u64; rook_total];
                let mut bishop_attacks = vec![0u64; bishop_total];

                for sq in 0..64u8 {
                    let rmask = rook_masks[sq as usize];
                    let bmask = bishop_masks[sq as usize];
                    let rshift = rook_shifts[sq as usize];
                    let bshift = bishop_shifts[sq as usize];
                    let roff = rook_offsets[sq as usize];
                    let boff = bishop_offsets[sq as usize];

                    let mut subset = rmask;
                    loop {
                        let idx =
                            ((subset & rmask).wrapping_mul(rook_magics[sq as usize]) >> rshift)
                                as usize;
                        rook_attacks[roff + idx] = rook_attacks_on_the_fly(sq, subset);
                        if subset == 0 {
                            break;
                        }
                        subset = (subset - 1) & rmask;
                    }

                    let mut subset = bmask;
                    loop {
                        let idx =
                            ((subset & bmask).wrapping_mul(bishop_magics[sq as usize]) >> bshift)
                                as usize;
                        bishop_attacks[boff + idx] = bishop_attacks_on_the_fly(sq, subset);
                        if subset == 0 {
                            break;
                        }
                        subset = (subset - 1) & bmask;
                    }
                }

                MagicTables {
                    rook_masks,
                    bishop_masks,
                    rook_magics,
                    bishop_magics,
                    rook_shifts,
                    bishop_shifts,
                    rook_offsets,
                    bishop_offsets,
                    rook_attacks,
                    bishop_attacks,
                }
            }
        }

        fn rook_mask(sq: u8) -> u64 {
            let f = file_of(sq) as i8;
            let r = rank_of(sq) as i8;
            let mut mask = 0u64;
            for df in [-1, 1] {
                let mut nf = f + df;
                while (1..=6).contains(&nf) {
                    mask |= 1u64 << (r as u8 * 8 + nf as u8);
                    nf += df;
                }
            }
            for dr in [-1, 1] {
                let mut nr = r + dr;
                while (1..=6).contains(&nr) {
                    mask |= 1u64 << (nr as u8 * 8 + f as u8);
                    nr += dr;
                }
            }
            mask
        }

        fn bishop_mask(sq: u8) -> u64 {
            let f = file_of(sq) as i8;
            let r = rank_of(sq) as i8;
            let mut mask = 0u64;
            for (df, dr) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let mut nf = f + df;
                let mut nr = r + dr;
                while (1..=6).contains(&nf) && (1..=6).contains(&nr) {
                    mask |= 1u64 << (nr as u8 * 8 + nf as u8);
                    nf += df;
                    nr += dr;
                }
            }
            mask
        }

        fn rook_attacks_on_the_fly(sq: u8, occ: u64) -> u64 {
            let f = file_of(sq) as i8;
            let r = rank_of(sq) as i8;
            let mut attacks = 0u64;
            for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let mut nf = f + df;
                let mut nr = r + dr;
                while (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                    let nsq = (nr as u8) * 8 + (nf as u8);
                    let bb = 1u64 << nsq;
                    attacks |= bb;
                    if (occ & bb) != 0 {
                        break;
                    }
                    nf += df;
                    nr += dr;
                }
            }
            attacks
        }

        fn bishop_attacks_on_the_fly(sq: u8, occ: u64) -> u64 {
            let f = file_of(sq) as i8;
            let r = rank_of(sq) as i8;
            let mut attacks = 0u64;
            for (df, dr) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let mut nf = f + df;
                let mut nr = r + dr;
                while (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                    let nsq = (nr as u8) * 8 + (nf as u8);
                    let bb = 1u64 << nsq;
                    attacks |= bb;
                    if (occ & bb) != 0 {
                        break;
                    }
                    nf += df;
                    nr += dr;
                }
            }
            attacks
        }
    }

    pub mod perft {
        use super::board::Position;
        use super::movegen;

        fn is_legal(pos: &mut Position, mv: super::mv::Move) -> bool {
            // Make the move
            pos.make_move(mv);

            // Find our king (we just moved, so we're now the other side)
            let our_side = pos.side_to_move.other();
            let us = our_side as usize;
            let king_bb = pos.pieces[us][super::types::Piece::King as usize];

            if king_bb == 0 {
                pos.unmake_move(mv);
                return false;
            }

            let king_sq = king_bb.trailing_zeros() as u8;

            // Check if our king is attacked by the current side to move
            let in_check = pos.is_square_attacked(king_sq, pos.side_to_move);

            // Unmake the move
            pos.unmake_move(mv);

            !in_check
        }

        pub fn perft(pos: &mut Position, depth: u8) -> u64 {
            if depth == 0 {
                return 1;
            }

            let moves = movegen::generate_moves(pos);

            if depth == 1 {
                // Leaf node optimization: just count legal moves
                let mut count = 0u64;
                for mv in moves {
                    if is_legal(pos, mv) {
                        count += 1;
                    }
                }
                return count;
            }

            let mut nodes = 0u64;
            for mv in moves {
                if !is_legal(pos, mv) {
                    continue;
                }

                pos.make_move(mv);
                nodes += perft(pos, depth - 1);
                pos.unmake_move(mv);
            }

            nodes
        }
    }

    pub mod eval {
        use super::board::Position;
        use super::movegen;
        use super::types::{file_of, rank_of, Piece, Side};
        use std::sync::OnceLock;

        const PHASE_WEIGHTS: [i32; 6] = [0, 1, 1, 2, 4, 0];
        const MAX_PHASE: i32 = 24;

        const PIECE_VALUES_MG: [i32; 6] = [100, 320, 330, 500, 900, 0];
        const PIECE_VALUES_EG: [i32; 6] = [120, 300, 320, 520, 900, 0];

        const PAWN_PST_MG: [i32; 64] = [
            0, 0, 0, 0, 0, 0, 0, 0, 50, 50, 50, 50, 50, 50, 50, 50, 10, 10, 20, 30, 30, 20, 10,
            10, 5, 5, 10, 25, 25, 10, 5, 5, 0, 0, 0, 20, 20, 0, 0, 0, 5, -5, -10, 0, 0, -10, -5,
            5, 5, 10, 10, -20, -20, 10, 10, 5, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        const PAWN_PST_EG: [i32; 64] = [
            0, 0, 0, 0, 0, 0, 0, 0, 60, 60, 60, 60, 60, 60, 60, 60, 20, 20, 30, 40, 40, 30, 20,
            20, 10, 10, 20, 30, 30, 20, 10, 10, 0, 0, 10, 20, 20, 10, 0, 0, 0, 0, 0, 10, 10, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        const KNIGHT_PST_MG: [i32; 64] = [
            -50, -40, -30, -30, -30, -30, -40, -50, -40, -20, 0, 0, 0, 0, -20, -40, -30, 0, 10,
            15, 15, 10, 0, -30, -30, 5, 15, 20, 20, 15, 5, -30, -30, 0, 15, 20, 20, 15, 0, -30,
            -30, 5, 10, 15, 15, 10, 5, -30, -40, -20, 0, 5, 5, 0, -20, -40, -50, -40, -30, -30,
            -30, -30, -40, -50,
        ];
        const KNIGHT_PST_EG: [i32; 64] = [
            -40, -20, -10, -10, -10, -10, -20, -40, -20, -5, 0, 5, 5, 0, -5, -20, -10, 5, 10, 15,
            15, 10, 5, -10, -10, 5, 15, 20, 20, 15, 5, -10, -10, 5, 15, 20, 20, 15, 5, -10, -10,
            0, 10, 15, 15, 10, 0, -10, -20, -5, 0, 5, 5, 0, -5, -20, -30, -20, -10, -10, -10, -10,
            -20, -30,
        ];
        const BISHOP_PST_MG: [i32; 64] = [
            -20, -10, -10, -10, -10, -10, -10, -20, -10, 0, 0, 0, 0, 0, 0, -10, -10, 0, 5, 10,
            10, 5, 0, -10, -10, 5, 5, 10, 10, 5, 5, -10, -10, 0, 10, 10, 10, 10, 0, -10, -10,
            10, 10, 10, 10, 10, -10, -10, 5, 0, 0, 0, 0, 5, -10, -20, -10, -10, -10, -10, -10,
            -10, -20, 0,
        ];
        const BISHOP_PST_EG: [i32; 64] = [
            -10, -5, -5, -5, -5, -5, -5, -10, -5, 5, 0, 0, 0, 0, 5, -5, -5, 0, 10, 10, 10, 10,
            0, -5, -5, 5, 10, 15, 15, 10, 5, -5, -5, 5, 10, 15, 15, 10, 5, -5, -5, 0, 10, 10, 10,
            10, 0, -5, -5, 0, 0, 0, 0, 0, -5, -10, -5, -5, -5, -5, -5, -5, -10, 0,
        ];
        const ROOK_PST_MG: [i32; 64] = [
            0, 0, 0, 0, 0, 0, 0, 0, 5, 10, 10, 10, 10, 10, 10, 5, -5, 0, 0, 0, 0, 0, 0, -5, -5,
            0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0,
            0, 0, 0, 0, -5, 0, 0, 0, 5, 5, 0, 0, 0,
        ];
        const ROOK_PST_EG: [i32; 64] = [
            0, 0, 5, 5, 5, 5, 0, 0, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0,
            0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0,
            0, 0, 0, -5, 0, 0, 0, 5, 5, 0, 0, 0,
        ];
        const QUEEN_PST_MG: [i32; 64] = [
            -20, -10, -10, -5, -5, -10, -10, -20, -10, 0, 0, 0, 0, 0, 0, -10, -10, 0, 5, 5, 5,
            5, 0, -10, -5, 0, 5, 5, 5, 5, 0, -5, 0, 0, 5, 5, 5, 5, 0, -5, -10, 5, 5, 5, 5, 5,
            0, -10, -10, 0, 5, 0, 0, 0, 0, -10, -20, -10, -10, -5, -5, -10, -10, -20,
        ];
        const QUEEN_PST_EG: [i32; 64] = [
            -10, -5, -5, -5, -5, -5, -5, -10, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 5, 5, 5, 5, 0, -5,
            -5, 0, 5, 10, 10, 5, 0, -5, -5, 0, 5, 10, 10, 5, 0, -5, -5, 0, 5, 5, 5, 5, 0, -5, -5,
            0, 0, 0, 0, 0, 0, -5, -10, -5, -5, -5, -5, -5, -5, -10,
        ];
        const KING_PST_MG: [i32; 64] = [
            -30, -40, -40, -50, -50, -40, -40, -30, -30, -40, -40, -50, -50, -40, -40, -30, -30, -40,
            -40, -50, -50, -40, -40, -30, -30, -40, -40, -50, -50, -40, -40, -30, -20, -30, -30,
            -40, -40, -30, -30, -20, -10, -20, -20, -20, -20, -20, -20, -10, 20, 20, 0, 0, 0, 0, 20,
            20, 20, 30, 10, 0, 0, 10, 30, 20,
        ];
        const KING_PST_EG: [i32; 64] = [
            -50, -40, -30, -20, -20, -30, -40, -50, -30, -20, -10, 0, 0, -10, -20, -30, -20, -10,
            10, 20, 20, 10, -10, -20, -20, -10, 20, 30, 30, 20, -10, -20, -20, -10, 20, 30, 30, 20,
            -10, -20, -20, -10, 10, 20, 20, 10, -10, -20, -30, -20, -10, 0, 0, -10, -20, -30, -50,
            -40, -30, -20, -20, -30, -40, -50,
        ];

        #[derive(Clone)]
        struct Nnue {
            input_weights: Vec<i16>,
            hidden_bias: [i16; 32],
            output_weights: [i16; 32],
            output_bias: i16,
        }

        static NNUE: OnceLock<Nnue> = OnceLock::new();

        pub fn evaluate(pos: &Position, use_nnue: bool) -> i32 {
            if use_nnue {
                return evaluate_nnue(pos);
            }
            evaluate_classic(pos)
        }

        fn evaluate_nnue(pos: &Position) -> i32 {
            let net = NNUE.get_or_init(Nnue::new);
            let mut hidden = [0i32; 32];
            for i in 0..32 {
                hidden[i] = net.hidden_bias[i] as i32;
            }

            for side in [Side::White, Side::Black] {
                let side_idx = side as usize;
                for piece_idx in 0..6 {
                    let mut bb = pos.pieces[side_idx][piece_idx];
                    while bb != 0 {
                        let sq = bb.trailing_zeros() as usize;
                        bb &= bb - 1;
                        let feat = (side_idx * 6 + piece_idx) * 64 + sq;
                        let base = feat * 32;
                        for h in 0..32 {
                            hidden[h] += net.input_weights[base + h] as i32;
                        }
                    }
                }
            }

            let mut out = net.output_bias as i32;
            for h in 0..32 {
                let v = hidden[h].max(0);
                out += v * net.output_weights[h] as i32;
            }

            let score = out / 64;
            if pos.side_to_move == Side::White {
                score
            } else {
                -score
            }
        }

        fn evaluate_classic(pos: &Position) -> i32 {
            let mut mg_score = 0;
            let mut eg_score = 0;

            for piece_idx in 0..6 {
                let piece = match piece_idx {
                    0 => Piece::Pawn,
                    1 => Piece::Knight,
                    2 => Piece::Bishop,
                    3 => Piece::Rook,
                    4 => Piece::Queen,
                    _ => Piece::King,
                };

                let mut white_bb = pos.pieces[Side::White as usize][piece_idx];
                while white_bb != 0 {
                    let sq = white_bb.trailing_zeros() as u8;
                    white_bb &= white_bb - 1;
                    mg_score += PIECE_VALUES_MG[piece_idx];
                    eg_score += PIECE_VALUES_EG[piece_idx];
                    let (mg, eg) = pst(piece, sq, Side::White);
                    mg_score += mg;
                    eg_score += eg;
                }

                let mut black_bb = pos.pieces[Side::Black as usize][piece_idx];
                while black_bb != 0 {
                    let sq = black_bb.trailing_zeros() as u8;
                    black_bb &= black_bb - 1;
                    mg_score -= PIECE_VALUES_MG[piece_idx];
                    eg_score -= PIECE_VALUES_EG[piece_idx];
                    let (mg, eg) = pst(piece, sq, Side::Black);
                    mg_score -= mg;
                    eg_score -= eg;
                }
            }

            let (mg_pawn, eg_pawn) = pawn_structure_eval(pos);
            mg_score += mg_pawn;
            eg_score += eg_pawn;

            let (mg_mob, eg_mob) = mobility_eval(pos);
            mg_score += mg_mob;
            eg_score += eg_mob;

            let (mg_king, eg_king) = king_safety_eval(pos);
            mg_score += mg_king;
            eg_score += eg_king;

            let phase = game_phase(pos);
            let mg = mg_score * phase;
            let eg = eg_score * (MAX_PHASE - phase);
            let score = (mg + eg) / MAX_PHASE;

            if pos.side_to_move == Side::White {
                score
            } else {
                -score
            }
        }

        fn pst(piece: Piece, sq: u8, side: Side) -> (i32, i32) {
            let idx = if side == Side::White {
                sq as usize
            } else {
                (63 - sq) as usize
            };
            match piece {
                Piece::Pawn => (PAWN_PST_MG[idx], PAWN_PST_EG[idx]),
                Piece::Knight => (KNIGHT_PST_MG[idx], KNIGHT_PST_EG[idx]),
                Piece::Bishop => (BISHOP_PST_MG[idx], BISHOP_PST_EG[idx]),
                Piece::Rook => (ROOK_PST_MG[idx], ROOK_PST_EG[idx]),
                Piece::Queen => (QUEEN_PST_MG[idx], QUEEN_PST_EG[idx]),
                Piece::King => (KING_PST_MG[idx], KING_PST_EG[idx]),
            }
        }

        fn game_phase(pos: &Position) -> i32 {
            let mut phase = 0;
            for piece_idx in 0..6 {
                let weight = PHASE_WEIGHTS[piece_idx];
                if weight == 0 {
                    continue;
                }
                let count = pos.pieces[Side::White as usize][piece_idx].count_ones()
                    + pos.pieces[Side::Black as usize][piece_idx].count_ones();
                phase += weight * (count as i32);
            }
            phase.clamp(0, MAX_PHASE)
        }

        fn pawn_structure_eval(pos: &Position) -> (i32, i32) {
            let mut mg = 0;
            let mut eg = 0;
            for side in [Side::White, Side::Black] {
                let us = side as usize;
                let them = side.other() as usize;
                let pawns = pos.pieces[us][Piece::Pawn as usize];
                let enemy_pawns = pos.pieces[them][Piece::Pawn as usize];

                let mut file_counts = [0u8; 8];
                let mut bb = pawns;
                while bb != 0 {
                    let sq = bb.trailing_zeros() as u8;
                    bb &= bb - 1;
                    file_counts[file_of(sq) as usize] += 1;
                }

                for f in 0..8 {
                    if file_counts[f] > 1 {
                        let penalty = 12 * ((file_counts[f] - 1) as i32);
                        mg -= penalty;
                        eg -= penalty;
                    }
                }

                let mut bb = pawns;
                while bb != 0 {
                    let sq = bb.trailing_zeros() as u8;
                    bb &= bb - 1;
                    let f = file_of(sq) as i8;
                    let r = rank_of(sq) as i8;

                    let has_left = if f > 0 {
                        file_counts[(f - 1) as usize] > 0
                    } else {
                        false
                    };
                    let has_right = if f < 7 {
                        file_counts[(f + 1) as usize] > 0
                    } else {
                        false
                    };

                    if !has_left && !has_right {
                        mg -= 12;
                        eg -= 8;
                    }

                    let (forward, promo_rank) = match side {
                        Side::White => (1i8, 7i8),
                        Side::Black => (-1i8, 0i8),
                    };
                    let mut passed = true;
                    let mut rr = r + forward;
                    while (0..=7).contains(&rr) {
                        for df in [-1, 0, 1] {
                            let nf = f + df;
                            if !(0..=7).contains(&nf) {
                                continue;
                            }
                            let sq_idx = (rr as u8) * 8 + (nf as u8);
                            if (enemy_pawns & (1u64 << sq_idx)) != 0 {
                                passed = false;
                            }
                        }
                        rr += forward;
                    }

                    if passed {
                        let advance = (promo_rank - r).abs() as i32;
                        mg += 10 + (6 - advance);
                        eg += 20 + (8 - advance) * 2;
                    }
                }
            }

            (mg, eg)
        }

        fn mobility_eval(pos: &Position) -> (i32, i32) {
            let mut mg = 0;
            let mut eg = 0;

            for side in [Side::White, Side::Black] {
                let mut temp = pos.clone();
                temp.side_to_move = side;
                let moves = movegen::generate_moves(&temp);
                let count = moves.len() as i32;
                let bonus = (count - 20).clamp(-10, 30);
                if side == Side::White {
                    mg += bonus;
                    eg += bonus / 2;
                } else {
                    mg -= bonus;
                    eg -= bonus / 2;
                }
            }

            (mg, eg)
        }

        fn king_safety_eval(pos: &Position) -> (i32, i32) {
            let mut mg = 0;
            let mut eg = 0;

            for side in [Side::White, Side::Black] {
                let us = side as usize;
                let king_bb = pos.pieces[us][Piece::King as usize];
                if king_bb == 0 {
                    continue;
                }
                let king_sq = king_bb.trailing_zeros() as u8;
                let f = file_of(king_sq) as i8;
                let r = rank_of(king_sq) as i8;
                let (forward, home_rank) = match side {
                    Side::White => (1i8, 0i8),
                    Side::Black => (-1i8, 7i8),
                };

                let mut shield = 0;
                for df in [-1, 0, 1] {
                    let nf = f + df;
                    if !(0..=7).contains(&nf) {
                        continue;
                    }
                    let rr = r + forward;
                    if !(0..=7).contains(&rr) {
                        continue;
                    }
                    let sq = (rr as u8) * 8 + (nf as u8);
                    if (pos.pieces[us][Piece::Pawn as usize] & (1u64 << sq)) != 0 {
                        shield += 1;
                    }
                }

                let missing = 3 - shield;
                let penalty = missing as i32 * 12;

                let mut open_files = 0;
                for df in [-1, 0, 1] {
                    let nf = f + df;
                    if !(0..=7).contains(&nf) {
                        continue;
                    }
                    let file_mask = 0x0101_0101_0101_0101u64 << (nf as u8);
                    if (pos.pieces[us][Piece::Pawn as usize] & file_mask) == 0 {
                        open_files += 1;
                    }
                }

                let open_penalty = open_files as i32 * 8;
                let home_bonus = if r == home_rank { 6 } else { 0 };

                if side == Side::White {
                    mg -= penalty + open_penalty - home_bonus;
                    eg -= (penalty / 2) - (home_bonus / 2);
                } else {
                    mg += penalty + open_penalty - home_bonus;
                    eg += (penalty / 2) - (home_bonus / 2);
                }
            }

            (mg, eg)
        }

        impl Nnue {
            fn new() -> Self {
                let mut rng = 0x1234_5678_9abc_def0u64;
                let mut next = || {
                    rng ^= rng << 13;
                    rng ^= rng >> 7;
                    rng ^= rng << 17;
                    rng
                };

                let mut input_weights = vec![0i16; 768 * 32];
                for w in input_weights.iter_mut() {
                    let v = (next() & 0xFF) as i16;
                    *w = (v as i16) - 128;
                }

                let mut hidden_bias = [0i16; 32];
                for b in hidden_bias.iter_mut() {
                    *b = ((next() & 0x7F) as i16) - 64;
                }

                let mut output_weights = [0i16; 32];
                for w in output_weights.iter_mut() {
                    *w = ((next() & 0xFF) as i16) - 128;
                }

                let output_bias = ((next() & 0xFF) as i16) - 128;

                Nnue {
                    input_weights,
                    hidden_bias,
                    output_weights,
                    output_bias,
                }
            }
        }
    }

    pub mod tablebase {
        use super::types::{file_of, rank_of, Piece, Side};
        use super::board::Position;
        use std::collections::{HashMap, HashSet};
        use std::sync::{Mutex, OnceLock};

        #[derive(Copy, Clone, PartialEq, Eq)]
        enum Outcome {
            Win,
            Loss,
            Draw,
        }

        struct KpkSolver {
            cache: HashMap<u64, Outcome>,
            in_progress: HashSet<u64>,
        }

        static KPK_SOLVER: OnceLock<Mutex<KpkSolver>> = OnceLock::new();

        pub fn probe(pos: &Position) -> Option<i32> {
            let (stm, wk, bk, pawn_sq, pawn_side) = extract_kpk(pos)?;
            let (stm, wk, bk, pawn_sq) = normalize(stm, wk, bk, pawn_sq, pawn_side);

            let outcome = {
                let mut solver = KPK_SOLVER
                    .get_or_init(|| Mutex::new(KpkSolver::new()))
                    .lock()
                    .unwrap();
                solver.solve(stm, wk, bk, pawn_sq)
            };

            let score = match outcome {
                Outcome::Win => 10000,
                Outcome::Loss => -10000,
                Outcome::Draw => 0,
            };
            Some(score)
        }

        impl KpkSolver {
            fn new() -> Self {
                KpkSolver {
                    cache: HashMap::new(),
                    in_progress: HashSet::new(),
                }
            }

            fn solve(&mut self, stm: Side, wk: u8, bk: u8, pawn_sq: u8) -> Outcome {
                let key = encode(stm, wk, bk, pawn_sq);
                if let Some(o) = self.cache.get(&key) {
                    return *o;
                }
                if self.in_progress.contains(&key) {
                    return Outcome::Draw;
                }
                self.in_progress.insert(key);

                let outcome = self.solve_inner(stm, wk, bk, pawn_sq);

                self.in_progress.remove(&key);
                self.cache.insert(key, outcome);
                outcome
            }

            fn solve_inner(&mut self, stm: Side, wk: u8, bk: u8, pawn_sq: u8) -> Outcome {
                if illegal_kings(wk, bk) {
                    return Outcome::Draw;
                }

                let pawn_rank = rank_of(pawn_sq);
                if pawn_rank == 0 || pawn_rank == 7 {
                    return Outcome::Win;
                }

                if stm == Side::White {
                    if can_promote(wk, bk, pawn_sq) {
                        return Outcome::Win;
                    }
                }

                let moves = generate_moves(stm, wk, bk, pawn_sq);
                if moves.is_empty() {
                    if in_check(stm, wk, bk, pawn_sq) {
                        return Outcome::Loss;
                    }
                    return Outcome::Draw;
                }

                let mut found_draw = false;
                for (n_wk, n_bk, n_pawn, n_stm) in moves {
                    let outcome = self.solve(n_stm, n_wk, n_bk, n_pawn);
                    match outcome {
                        Outcome::Loss => {
                            return Outcome::Win;
                        }
                        Outcome::Draw => {
                            found_draw = true;
                        }
                        Outcome::Win => {}
                    }
                }

                if found_draw {
                    Outcome::Draw
                } else {
                    Outcome::Loss
                }
            }
        }

        fn extract_kpk(pos: &Position) -> Option<(Side, u8, u8, u8, Side)> {
            let mut total = 0;
            for s in 0..2 {
                for p in 0..6 {
                    total += pos.pieces[s][p].count_ones();
                }
            }
            if total != 3 {
                return None;
            }

            let wk_bb = pos.pieces[Side::White as usize][Piece::King as usize];
            let bk_bb = pos.pieces[Side::Black as usize][Piece::King as usize];
            if wk_bb == 0 || bk_bb == 0 {
                return None;
            }
            let wk = wk_bb.trailing_zeros() as u8;
            let bk = bk_bb.trailing_zeros() as u8;

            let white_pawns = pos.pieces[Side::White as usize][Piece::Pawn as usize];
            let black_pawns = pos.pieces[Side::Black as usize][Piece::Pawn as usize];
            let pawn_side = if white_pawns != 0 {
                Side::White
            } else if black_pawns != 0 {
                Side::Black
            } else {
                return None;
            };
            let pawn_sq = if pawn_side == Side::White {
                white_pawns.trailing_zeros() as u8
            } else {
                black_pawns.trailing_zeros() as u8
            };

            let extra_white = pos.pieces[Side::White as usize][Piece::Knight as usize]
                | pos.pieces[Side::White as usize][Piece::Bishop as usize]
                | pos.pieces[Side::White as usize][Piece::Rook as usize]
                | pos.pieces[Side::White as usize][Piece::Queen as usize];
            let extra_black = pos.pieces[Side::Black as usize][Piece::Knight as usize]
                | pos.pieces[Side::Black as usize][Piece::Bishop as usize]
                | pos.pieces[Side::Black as usize][Piece::Rook as usize]
                | pos.pieces[Side::Black as usize][Piece::Queen as usize];
            if extra_white != 0 || extra_black != 0 {
                return None;
            }

            Some((pos.side_to_move, wk, bk, pawn_sq, pawn_side))
        }

        fn normalize(stm: Side, wk: u8, bk: u8, pawn_sq: u8, pawn_side: Side) -> (Side, u8, u8, u8) {
            if pawn_side == Side::White {
                (stm, wk, bk, pawn_sq)
            } else {
                let wk_n = mirror_sq(bk);
                let bk_n = mirror_sq(wk);
                let pawn_n = mirror_sq(pawn_sq);
                let stm_n = if stm == Side::Black { Side::White } else { Side::Black };
                (stm_n, wk_n, bk_n, pawn_n)
            }
        }

        fn mirror_sq(sq: u8) -> u8 {
            let f = file_of(sq);
            let r = rank_of(sq);
            (7 - r) * 8 + f
        }

        fn encode(stm: Side, wk: u8, bk: u8, pawn_sq: u8) -> u64 {
            (stm as u64)
                | ((wk as u64) << 1)
                | ((bk as u64) << 7)
                | ((pawn_sq as u64) << 13)
        }

        fn illegal_kings(wk: u8, bk: u8) -> bool {
            let wf = file_of(wk) as i8;
            let wr = rank_of(wk) as i8;
            let bf = file_of(bk) as i8;
            let br = rank_of(bk) as i8;
            (wf - bf).abs() <= 1 && (wr - br).abs() <= 1
        }

        fn in_check(stm: Side, wk: u8, bk: u8, pawn_sq: u8) -> bool {
            match stm {
                Side::Black => white_attacks(wk, pawn_sq, bk),
                Side::White => black_attacks(wk, bk),
            }
        }

        fn white_attacks(wk: u8, pawn_sq: u8, target: u8) -> bool {
            if adjacent(wk, target) {
                return true;
            }
            let f = file_of(pawn_sq) as i8;
            let r = rank_of(pawn_sq) as i8;
            for df in [-1, 1] {
                let nf = f + df;
                let nr = r + 1;
                if (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                    let sq = (nr as u8) * 8 + (nf as u8);
                    if sq == target {
                        return true;
                    }
                }
            }
            false
        }

        fn black_attacks(wk: u8, bk: u8) -> bool {
            adjacent(wk, bk)
        }

        fn adjacent(a: u8, b: u8) -> bool {
            let af = file_of(a) as i8;
            let ar = rank_of(a) as i8;
            let bf = file_of(b) as i8;
            let br = rank_of(b) as i8;
            (af - bf).abs() <= 1 && (ar - br).abs() <= 1
        }

        fn can_promote(wk: u8, bk: u8, pawn_sq: u8) -> bool {
            let rank = rank_of(pawn_sq);
            if rank != 6 {
                return false;
            }
            let f = file_of(pawn_sq) as i8;
            let target = (7u8) * 8 + (f as u8);
            if target == wk || target == bk {
                return false;
            }
            if adjacent(bk, target) {
                return false;
            }
            true
        }

        fn generate_moves(stm: Side, wk: u8, bk: u8, pawn_sq: u8) -> Vec<(u8, u8, u8, Side)> {
            let mut moves = Vec::new();
            match stm {
                Side::White => {
                    for (df, dr) in [
                        (1, 0),
                        (-1, 0),
                        (0, 1),
                        (0, -1),
                        (1, 1),
                        (1, -1),
                        (-1, 1),
                        (-1, -1),
                    ] {
                        let nf = file_of(wk) as i8 + df;
                        let nr = rank_of(wk) as i8 + dr;
                        if !(0..=7).contains(&nf) || !(0..=7).contains(&nr) {
                            continue;
                        }
                        let nsq = (nr as u8) * 8 + (nf as u8);
                        if nsq == pawn_sq {
                            continue;
                        }
                        if adjacent(nsq, bk) {
                            continue;
                        }
                        moves.push((nsq, bk, pawn_sq, Side::Black));
                    }

                    let r = rank_of(pawn_sq) as i8;
                    let f = file_of(pawn_sq) as i8;
                    let forward = r + 1;
                    if forward <= 7 {
                        let to = (forward as u8) * 8 + (f as u8);
                        if to != wk && to != bk {
                            moves.push((wk, bk, to, Side::Black));
                            if r == 1 {
                                let to2 = ((r + 2) as u8) * 8 + (f as u8);
                                if to2 != wk && to2 != bk {
                                    moves.push((wk, bk, to2, Side::Black));
                                }
                            }
                        }
                    }
                }
                Side::Black => {
                    for (df, dr) in [
                        (1, 0),
                        (-1, 0),
                        (0, 1),
                        (0, -1),
                        (1, 1),
                        (1, -1),
                        (-1, 1),
                        (-1, -1),
                    ] {
                        let nf = file_of(bk) as i8 + df;
                        let nr = rank_of(bk) as i8 + dr;
                        if !(0..=7).contains(&nf) || !(0..=7).contains(&nr) {
                            continue;
                        }
                        let nsq = (nr as u8) * 8 + (nf as u8);
                        if nsq == wk {
                            continue;
                        }
                        if adjacent(nsq, wk) {
                            continue;
                        }
                        if white_attacks(wk, pawn_sq, nsq) {
                            continue;
                        }
                        moves.push((wk, nsq, pawn_sq, Side::White));
                    }
                }
            }
            moves
        }
    }

    pub mod search {
        use super::board::Position;
        use super::eval;
        use super::movegen;
        use super::mv::Move;
        use super::types::{Piece, Side};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::thread;
        use std::time::{Duration, Instant};

        const MATE_SCORE: i32 = 30000;
        const MAX_PLY: u8 = 64;
        const TT_SIZE: usize = 1048576; // 1M entries (adjust based on memory)
        const HISTORY_SIZE: usize = 64 * 64; // from_sq * to_sq
        const NODE_CHECK_INTERVAL: u64 = 2048;

        #[derive(Clone, Copy)]
        pub struct SearchOptions {
            pub use_nnue: bool,
            pub use_tablebase: bool,
            pub threads: usize,
        }

        impl Default for SearchOptions {
            fn default() -> Self {
                Self {
                    use_nnue: true,
                    use_tablebase: true,
                    threads: 1,
                }
            }
        }

        pub struct SearchInfo {
            pub depth: u8,
            pub score: i32,
            pub nodes: u64,
            pub time_ms: u64,
            pub pv: Vec<Move>,
        }

        pub struct SearchReport {
            pub best_move: Move,
            pub best_score: i32,
            pub depth: u8,
            pub nodes: u64,
            pub time_ms: u64,
            pub pv: Vec<Move>,
            pub infos: Vec<SearchInfo>,
        }

        #[derive(Clone)]
        struct KillerMoves {
            killers: [[Move; 2]; MAX_PLY as usize],
        }

        impl KillerMoves {
            fn new() -> Self {
                KillerMoves {
                    killers: [[Move::NONE; 2]; MAX_PLY as usize],
                }
            }

            fn add(&mut self, ply: u8, mv: Move) {
                let ply_idx = ply as usize;
                if ply_idx >= MAX_PLY as usize {
                    return;
                }
                // Shift killers: killer[0] becomes killer[1], new move becomes killer[0]
                if self.killers[ply_idx][0].0 != mv.0 {
                    self.killers[ply_idx][1] = self.killers[ply_idx][0];
                    self.killers[ply_idx][0] = mv;
                }
            }

            fn is_killer(&self, ply: u8, mv: Move) -> bool {
                let ply_idx = ply as usize;
                if ply_idx >= MAX_PLY as usize {
                    return false;
                }
                self.killers[ply_idx][0].0 == mv.0 || self.killers[ply_idx][1].0 == mv.0
            }
        }

        struct History {
            scores: [i32; HISTORY_SIZE],
        }

        impl History {
            fn new() -> Self {
                History {
                    scores: [0; HISTORY_SIZE],
                }
            }

            fn update(&mut self, mv: Move, depth: u8) {
                let idx = (mv.from() as usize) * 64 + (mv.to() as usize);
                self.scores[idx] += (depth as i32) * (depth as i32);
            }

            fn score(&self, mv: Move) -> i32 {
                let idx = (mv.from() as usize) * 64 + (mv.to() as usize);
                self.scores[idx]
            }

            fn clear(&mut self) {
                self.scores = [0; HISTORY_SIZE];
            }
        }

        #[derive(Clone)]
        struct PVTable {
            pv: [[Move; MAX_PLY as usize]; MAX_PLY as usize],
            pv_len: [u8; MAX_PLY as usize],
        }

        impl PVTable {
            fn new() -> Self {
                PVTable {
                    pv: [[Move::NONE; MAX_PLY as usize]; MAX_PLY as usize],
                    pv_len: [0; MAX_PLY as usize],
                }
            }

            fn update(&mut self, ply: u8, mv: Move) {
                let p = ply as usize;
                self.pv[p][0] = mv;
                let next_len = if p + 1 < MAX_PLY as usize {
                    self.pv_len[p + 1] as usize
                } else {
                    0
                };
                for i in 0..next_len {
                    self.pv[p][i + 1] = self.pv[p + 1][i];
                }
                self.pv_len[p] = (next_len + 1) as u8;
            }

            fn line(&self) -> Vec<Move> {
                let len = self.pv_len[0] as usize;
                self.pv[0][0..len].to_vec()
            }

            fn set_line(&mut self, line: &[Move]) {
                let len = line.len().min(MAX_PLY as usize);
                self.pv_len[0] = len as u8;
                for i in 0..len {
                    self.pv[0][i] = line[i];
                }
            }
        }

        #[derive(Clone, Default)]
        struct SearchStats {
            nodes: u64,
        }

        #[derive(Clone)]
        pub struct TimeManager {
            start_time: Instant,
            soft_limit: Duration,
            hard_limit: Duration,
            stop_flag: Arc<AtomicBool>,
        }

        impl TimeManager {
            pub fn new(allocated_ms: u64) -> Self {
                let allocated = Duration::from_millis(allocated_ms.max(1));
                TimeManager {
                    start_time: Instant::now(),
                    soft_limit: allocated,
                    hard_limit: allocated * 3,
                    stop_flag: Arc::new(AtomicBool::new(false)),
                }
            }

            pub fn with_limits(soft_ms: u64, hard_ms: u64) -> Self {
                TimeManager {
                    start_time: Instant::now(),
                    soft_limit: Duration::from_millis(soft_ms.max(1)),
                    hard_limit: Duration::from_millis(hard_ms.max(soft_ms).max(1)),
                    stop_flag: Arc::new(AtomicBool::new(false)),
                }
            }

            pub fn infinite() -> Self {
                TimeManager {
                    start_time: Instant::now(),
                    soft_limit: Duration::from_secs(9999),
                    hard_limit: Duration::from_secs(9999),
                    stop_flag: Arc::new(AtomicBool::new(false)),
                }
            }

            pub fn should_stop(&self) -> bool {
                if self.stop_flag.load(Ordering::Relaxed) {
                    return true;
                }
                let elapsed = self.start_time.elapsed();
                if elapsed >= self.hard_limit {
                    self.stop_flag.store(true, Ordering::Relaxed);
                    true
                } else if elapsed >= self.soft_limit {
                    self.stop_flag.store(true, Ordering::Relaxed);
                    true
                } else {
                    false
                }
            }

            pub fn elapsed(&self) -> Duration {
                self.start_time.elapsed()
            }

            pub fn stop(&self) {
                self.stop_flag.store(true, Ordering::Relaxed);
            }
        }

        #[derive(Copy, Clone, PartialEq)]
        enum Bound {
            Exact,
            Lower,
            Upper,
        }

        #[derive(Copy, Clone)]
        struct TTEntry {
            key: u64,
            depth: u8,
            score: i32,
            bound: Bound,
            best_move: Move,
        }

        impl TTEntry {
            fn empty() -> Self {
                TTEntry {
                    key: 0,
                    depth: 0,
                    score: 0,
                    bound: Bound::Exact,
                    best_move: Move::NONE,
                }
            }
        }

        pub struct TranspositionTable {
            entries: Vec<TTEntry>,
        }

        impl TranspositionTable {
            pub fn new() -> Self {
                TranspositionTable {
                    entries: vec![TTEntry::empty(); TT_SIZE],
                }
            }

            fn probe(&self, key: u64, depth: u8, alpha: i32, beta: i32) -> Option<(i32, Move)> {
                let index = (key as usize) % TT_SIZE;
                let entry = self.entries[index];

                if entry.key == key && entry.depth >= depth {
                    match entry.bound {
                        Bound::Exact => return Some((entry.score, entry.best_move)),
                        Bound::Lower if entry.score >= beta => {
                            return Some((entry.score, entry.best_move))
                        }
                        Bound::Upper if entry.score <= alpha => {
                            return Some((entry.score, entry.best_move))
                        }
                        _ => {}
                    }
                }

                // Return best move even if score not usable
                if entry.key == key && entry.best_move != Move::NONE {
                    return Some((0, entry.best_move));
                }

                None
            }

            fn store(&mut self, key: u64, depth: u8, score: i32, bound: Bound, best_move: Move) {
                let index = (key as usize) % TT_SIZE;
                let entry = &mut self.entries[index];

                // Replace if deeper or same position
                if entry.key != key || entry.depth <= depth {
                    *entry = TTEntry {
                        key,
                        depth,
                        score,
                        bound,
                        best_move,
                    };
                }
            }
        }

        // Piece values in centipawns (MVV-LVA only)
        const PIECE_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 0];

        fn mvv_lva_score(pos: &Position, mv: Move) -> i32 {
            let to = mv.to();
            let to_bb = 1u64 << to;
            let them = pos.side_to_move.other() as usize;

            // Check if capture
            if (pos.occ[them] & to_bb) == 0 {
                return 0; // Not a capture
            }

            // Find victim piece
            let victim_value = (0..6)
                .find(|&p| (pos.pieces[them][p] & to_bb) != 0)
                .map(|p| PIECE_VALUES[p])
                .unwrap_or(0);

            // Find attacker piece
            let from = mv.from();
            let from_bb = 1u64 << from;
            let us = pos.side_to_move as usize;
            let attacker_value = (0..6)
                .find(|&p| (pos.pieces[us][p] & from_bb) != 0)
                .map(|p| PIECE_VALUES[p])
                .unwrap_or(0);

            // MVV-LVA: prioritize high-value victims and low-value attackers
            victim_value * 10 - attacker_value
        }

        fn order_moves(
            moves: &mut Vec<Move>,
            pos: &Position,
            tt_move: Move,
            killers: &KillerMoves,
            history: &History,
            ply: u8,
        ) {
            // Score each move for ordering
            let mut move_scores: Vec<(Move, i32)> = moves
                .iter()
                .map(|&mv| {
                    let mut score = 0;

                    // TT move gets highest priority
                    if mv.0 == tt_move.0 {
                        score = 1_000_000;
                    } else {
                        // Check if it's a capture
                        let to = mv.to();
                        let to_bb = 1u64 << to;
                        let them = pos.side_to_move.other() as usize;
                        if (pos.occ[them] & to_bb) != 0 {
                            // Captures: MVV-LVA score
                            score = 10_000 + mvv_lva_score(pos, mv);
                        } else {
                            // Quiet moves: killers and history
                            if killers.is_killer(ply, mv) {
                                score = 9_000; // High priority for killers
                            } else {
                                score = history.score(mv);
                            }
                        }
                    }

                    (mv, score)
                })
                .collect();

            // Sort by score descending
            move_scores.sort_by(|a, b| b.1.cmp(&a.1));

            // Update moves vector
            for (i, (mv, _)) in move_scores.into_iter().enumerate() {
                moves[i] = mv;
            }
        }

        fn evaluate(pos: &Position, options: &SearchOptions) -> i32 {
            eval::evaluate(pos, options.use_nnue)
        }

        fn is_in_check(pos: &Position) -> bool {
            let us = pos.side_to_move as usize;
            let king_bb = pos.pieces[us][Piece::King as usize];
            if king_bb == 0 {
                return false;
            }
            let king_sq = king_bb.trailing_zeros() as u8;
            pos.is_square_attacked(king_sq, pos.side_to_move.other())
        }

        fn is_legal(pos: &mut Position, mv: Move) -> bool {
            pos.make_move(mv);
            let our_side = pos.side_to_move.other();
            let us = our_side as usize;
            let king_bb = pos.pieces[us][Piece::King as usize];

            if king_bb == 0 {
                pos.unmake_move(mv);
                return false;
            }

            let king_sq = king_bb.trailing_zeros() as u8;
            let in_check = pos.is_square_attacked(king_sq, pos.side_to_move);
            pos.unmake_move(mv);
            !in_check
        }

        struct SearchContext<'a> {
            tt: &'a mut TranspositionTable,
            killers: &'a mut KillerMoves,
            history: &'a mut History,
            pv: &'a mut PVTable,
            stats: &'a mut SearchStats,
            options: &'a SearchOptions,
            time: &'a TimeManager,
        }

        fn quiescence(
            pos: &mut Position,
            mut alpha: i32,
            beta: i32,
            ctx: &mut SearchContext<'_>,
        ) -> i32 {
            ctx.stats.nodes += 1;
            if (ctx.stats.nodes % NODE_CHECK_INTERVAL) == 0 && ctx.time.should_stop() {
                return alpha;
            }

            let stand_pat = evaluate(pos, ctx.options);

            if stand_pat >= beta {
                return beta;
            }
            if alpha < stand_pat {
                alpha = stand_pat;
            }

            // Generate and search only captures
            let moves = movegen::generate_moves(pos);
            for mv in moves {
                // Simple capture detection: check if destination has enemy piece
                let to = mv.to();
                let to_bb = 1u64 << to;
                let them = pos.side_to_move.other() as usize;
                let is_capture = (pos.occ[them] & to_bb) != 0;

                if !is_capture {
                    continue;
                }

                if !is_legal(pos, mv) {
                    continue;
                }

                pos.make_move(mv);
                let score = -quiescence(pos, -beta, -alpha, ctx);
                pos.unmake_move(mv);

                if score >= beta {
                    return beta;
                }
                if score > alpha {
                    alpha = score;
                }
            }

            alpha
        }

        fn alpha_beta(
            pos: &mut Position,
            depth: u8,
            mut alpha: i32,
            beta: i32,
            ply: u8,
            ctx: &mut SearchContext<'_>,
            do_null: bool,
        ) -> (i32, Move) {
            // Check time
            ctx.stats.nodes += 1;
            if (ctx.stats.nodes % NODE_CHECK_INTERVAL) == 0 && ctx.time.should_stop() {
                return (alpha, Move::NONE);
            }
            let original_alpha = alpha;
            let mut best_move = Move::NONE;

            // TT probe
            if let Some((tt_score, tt_move)) = ctx.tt.probe(pos.zobrist_key, depth, alpha, beta) {
                if depth > 0 && tt_score != 0 {
                    return (tt_score, tt_move);
                }
                best_move = tt_move; // Use TT move for move ordering
            }

            if ctx.options.use_tablebase {
                if let Some(tb_score) = super::tablebase::probe(pos) {
                    return (tb_score, Move::NONE);
                }
            }

            if depth == 0 {
                let score = quiescence(pos, alpha, beta, ctx);
                return (score, Move::NONE);
            }

            if ply >= MAX_PLY {
                return (evaluate(pos, ctx.options), Move::NONE);
            }

            // Null-move pruning
            if do_null && depth >= 3 && !is_in_check(pos) {
                // Don't do null move in endgame with only pawns (zugzwang risk)
                let us = pos.side_to_move as usize;
                let non_pawn_material = pos.pieces[us][Piece::Knight as usize].count_ones()
                    + pos.pieces[us][Piece::Bishop as usize].count_ones()
                    + pos.pieces[us][Piece::Rook as usize].count_ones()
                    + pos.pieces[us][Piece::Queen as usize].count_ones();

                if non_pawn_material > 0 {
                    // Make null move (just flip side to move)
                    pos.side_to_move = pos.side_to_move.other();
                    pos.zobrist_key ^= super::zobrist::keys().side;

                    let r = 3; // Reduction factor
                    let (score, _) = alpha_beta(
                        pos,
                        depth.saturating_sub(r),
                        -beta,
                        -beta + 1,
                        ply + 1,
                        ctx,
                        false, // Don't allow consecutive null moves
                    );
                    let score = -score;

                    // Unmake null move
                    pos.side_to_move = pos.side_to_move.other();
                    pos.zobrist_key ^= super::zobrist::keys().side;

                    if score >= beta {
                        return (beta, Move::NONE);
                    }
                }
            }

            let mut moves = movegen::generate_moves(pos);

            // Move ordering with TT, killers, and history
            order_moves(&mut moves, pos, best_move, ctx.killers, ctx.history, ply);

            let mut legal_moves = 0;

            for (move_num, mv) in moves.iter().enumerate() {
                let mv = *mv;
                if !is_legal(pos, mv) {
                    continue;
                }

                legal_moves += 1;

                // Determine if this is a quiet move for LMR
                let to = mv.to();
                let to_bb = 1u64 << to;
                let them = pos.side_to_move.other() as usize;
                let is_capture = (pos.occ[them] & to_bb) != 0;
                let is_promotion = mv.promo().is_some();
                let is_killer = ctx.killers.is_killer(ply, mv);

                pos.make_move(mv);
                let gives_check = is_in_check(pos);
                pos.unmake_move(mv);

                let mut score;

                // Late Move Reductions (LMR)
                if legal_moves >= 4
                    && depth >= 3
                    && !is_capture
                    && !is_promotion
                    && !is_killer
                    && !gives_check
                    && move_num > 0
                {
                    // Reduce depth for late quiet moves
                    let reduction = if legal_moves >= 6 && depth >= 5 { 2 } else { 1 };

                    pos.make_move(mv);
                    let (reduced_score, _) = alpha_beta(
                        pos,
                        depth.saturating_sub(1 + reduction),
                        -alpha - 1,
                        -alpha,
                        ply + 1,
                        ctx,
                        true,
                    );
                    score = -reduced_score;

                    // If reduced search fails high, re-search at full depth
                    if score > alpha {
                        let (full_score, _) = alpha_beta(
                            pos,
                            depth - 1,
                            -beta,
                            -alpha,
                            ply + 1,
                            ctx,
                            true,
                        );
                        score = -full_score;
                    }

                    pos.unmake_move(mv);
                } else {
                    // Full depth search
                    pos.make_move(mv);
                    let (full_score, _) = alpha_beta(
                        pos,
                        depth - 1,
                        -beta,
                        -alpha,
                        ply + 1,
                        ctx,
                        true,
                    );
                    score = -full_score;
                    pos.unmake_move(mv);
                }

                if score >= beta {
                    ctx.tt.store(pos.zobrist_key, depth, beta, Bound::Lower, mv);

                    // Update killer moves and history for quiet moves
                    let to = mv.to();
                    let to_bb = 1u64 << to;
                    let them = pos.side_to_move.other() as usize;
                    let is_quiet = (pos.occ[them] & to_bb) == 0;

                    if is_quiet {
                        ctx.killers.add(ply, mv);
                        ctx.history.update(mv, depth);
                    }

                    return (beta, mv);
                }
                if score > alpha {
                    alpha = score;
                    best_move = mv;
                    ctx.pv.update(ply, mv);
                }
            }

            // Checkmate or stalemate
            if legal_moves == 0 {
                let king = pos.pieces[pos.side_to_move as usize][Piece::King as usize];
                if king != 0 {
                    let king_sq = king.trailing_zeros() as u8;
                    let in_check = pos.is_square_attacked(king_sq, pos.side_to_move.other());
                    if in_check {
                        ctx.tt.store(
                            pos.zobrist_key,
                            depth,
                            -MATE_SCORE + (ply as i32),
                            Bound::Exact,
                            Move::NONE,
                        );
                        return (-MATE_SCORE + (ply as i32), Move::NONE); // Checkmate
                    }
                }
                ctx.tt.store(pos.zobrist_key, depth, 0, Bound::Exact, Move::NONE);
                return (0, Move::NONE); // Stalemate
            }

            // Store in TT
            let bound = if alpha <= original_alpha {
                Bound::Upper
            } else if alpha >= beta {
                Bound::Lower
            } else {
                Bound::Exact
            };
            ctx.tt.store(pos.zobrist_key, depth, alpha, bound, best_move);

            (alpha, best_move)
        }

        fn search_root_single(
            pos: &mut Position,
            depth: u8,
            alpha: i32,
            beta: i32,
            time_manager: &TimeManager,
            options: &SearchOptions,
            tt: &mut TranspositionTable,
            killers: &mut KillerMoves,
            history: &mut History,
            pv: &mut PVTable,
            stats: &mut SearchStats,
        ) -> (i32, Move, Vec<Move>) {
            let mut ctx = SearchContext {
                tt,
                killers,
                history,
                pv,
                stats,
                options,
                time: time_manager,
            };

            let mut best_move = Move::NONE;
            let mut best_score = -MATE_SCORE - 1;
            let mut a = alpha;
            let b = beta;

            let moves = movegen::generate_moves(pos);
            for mv in moves {
                if !is_legal(pos, mv) {
                    continue;
                }

                pos.make_move(mv);
                let (score, _) = alpha_beta(
                    pos,
                    depth - 1,
                    -b,
                    -a,
                    1,
                    &mut ctx,
                    true,
                );
                let score = -score;
                pos.unmake_move(mv);

                if score > best_score {
                    best_score = score;
                    best_move = mv;
                }

                if score > a {
                    a = score;
                    ctx.pv.update(0, mv);
                }

                if time_manager.should_stop() {
                    break;
                }
            }

            let pv_line = ctx.pv.line();
            (best_score, best_move, pv_line)
        }

        fn search_root_parallel(
            pos: &mut Position,
            depth: u8,
            alpha: i32,
            beta: i32,
            time_manager: &TimeManager,
            options: &SearchOptions,
        ) -> (i32, Move, Vec<Move>, u64) {
            let moves = movegen::generate_moves(pos);
            if moves.is_empty() {
                return (0, Move::NONE, Vec::new(), 0);
            }

            let threads = options.threads.min(moves.len()).max(1);
            let mut handles = Vec::new();
            let mut total_nodes = 0u64;

            for tid in 0..threads {
                let chunk: Vec<Move> = moves
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| i % threads == tid)
                    .map(|(_, m)| *m)
                    .collect();
                let mut pos_clone = pos.clone();
                let tm = time_manager.clone();
                let opts = *options;

                let handle = thread::spawn(move || {
                    let mut tt = TranspositionTable::new();
                    let mut killers = KillerMoves::new();
                    let mut history = History::new();
                    let mut pv = PVTable::new();
                    let mut stats = SearchStats::default();

                    let mut best_move = Move::NONE;
                    let mut best_score = -MATE_SCORE - 1;
                    let mut a = alpha;
                    let b = beta;

                    let mut ctx = SearchContext {
                        tt: &mut tt,
                        killers: &mut killers,
                        history: &mut history,
                        pv: &mut pv,
                        stats: &mut stats,
                        options: &opts,
                        time: &tm,
                    };

                    for mv in chunk {
                        if !is_legal(&mut pos_clone, mv) {
                            continue;
                        }
                        pos_clone.make_move(mv);
                        let (score, _) = alpha_beta(
                            &mut pos_clone,
                            depth - 1,
                            -b,
                            -a,
                            1,
                            &mut ctx,
                            true,
                        );
                        let score = -score;
                        pos_clone.unmake_move(mv);

                        if score > best_score {
                            best_score = score;
                            best_move = mv;
                        }
                        if score > a {
                            a = score;
                            ctx.pv.update(0, mv);
                        }

                        if tm.should_stop() {
                            break;
                        }
                    }

                    let pv_line = ctx.pv.line();
                    (best_score, best_move, pv_line, stats.nodes)
                });
                handles.push(handle);
            }

            let mut best_score = -MATE_SCORE - 1;
            let mut best_move = Move::NONE;
            let mut best_pv = Vec::new();

            for h in handles {
                if let Ok((score, mv, pv_line, nodes)) = h.join() {
                    total_nodes += nodes;
                    if score > best_score {
                        best_score = score;
                        best_move = mv;
                        best_pv = pv_line;
                    }
                }
            }

            (best_score, best_move, best_pv, total_nodes)
        }

        pub fn search_position(
            pos: &mut Position,
            max_depth: u8,
            time_manager: &TimeManager,
            options: SearchOptions,
        ) -> SearchReport {
            let mut tt = TranspositionTable::new();
            let mut killers = KillerMoves::new();
            let mut history = History::new();
            let mut pv = PVTable::new();
            let mut stats = SearchStats::default();
            let mut infos = Vec::new();

            let mut best_move = Move::NONE;
            let mut best_score = -MATE_SCORE - 1;
            let mut last_score = 0;

            for depth in 1..=max_depth {
                if time_manager.should_stop() {
                    break;
                }

                let mut alpha = -MATE_SCORE - 1;
                let mut beta = MATE_SCORE + 1;

                if depth > 1 {
                    let window = 50;
                    alpha = (last_score - window).max(-MATE_SCORE);
                    beta = (last_score + window).min(MATE_SCORE);
                }

                let (score, mv, pv_line, nodes) = if options.threads > 1 && depth >= 4 {
                    search_root_parallel(pos, depth, alpha, beta, time_manager, &options)
                } else {
                    let (s, m, line) = search_root_single(
                        pos,
                        depth,
                        alpha,
                        beta,
                        time_manager,
                        &options,
                        &mut tt,
                        &mut killers,
                        &mut history,
                        &mut pv,
                        &mut stats,
                    );
                    (s, m, line, stats.nodes)
                };

                if options.threads > 1 && depth >= 4 {
                    stats.nodes += nodes;
                    pv.set_line(&pv_line);
                }

                let mut final_score = score;
                let mut final_move = mv;
                let mut final_pv = pv_line;

                if final_score <= alpha || final_score >= beta {
                    let mut window = 150;
                    loop {
                        let a = (last_score - window).max(-MATE_SCORE);
                        let b = (last_score + window).min(MATE_SCORE);
                        let (s, m, line, nodes) = if options.threads > 1 && depth >= 4 {
                            search_root_parallel(pos, depth, a, b, time_manager, &options)
                        } else {
                            let (s, m, line) = search_root_single(
                                pos,
                                depth,
                                a,
                                b,
                                time_manager,
                                &options,
                                &mut tt,
                                &mut killers,
                                &mut history,
                                &mut pv,
                                &mut stats,
                            );
                            (s, m, line, stats.nodes)
                        };
                        if options.threads > 1 && depth >= 4 {
                            stats.nodes += nodes;
                            pv.set_line(&line);
                        }
                        final_score = s;
                        final_move = m;
                        final_pv = line;
                        if final_score > a && final_score < b {
                            break;
                        }
                        if time_manager.should_stop() {
                            break;
                        }
                        window = (window * 2).min(2000);
                    }
                }

                best_score = final_score;
                best_move = final_move;
                last_score = best_score;

                let pv_line = if options.threads > 1 && depth >= 4 {
                    final_pv.clone()
                } else {
                    pv.line()
                };
                infos.push(SearchInfo {
                    depth,
                    score: best_score,
                    nodes: stats.nodes,
                    time_ms: time_manager.elapsed().as_millis() as u64,
                    pv: pv_line.clone(),
                });

                killers = KillerMoves::new();
            }

            SearchReport {
                best_move,
                best_score,
                depth: infos.last().map(|i| i.depth).unwrap_or(0),
                nodes: stats.nodes,
                time_ms: time_manager.elapsed().as_millis() as u64,
                pv: pv.line(),
                infos,
            }
        }

        pub fn calculate_time(our_time: u64, our_inc: u64, moves_to_go: Option<u64>) -> u64 {
            let overhead = 30u64;
            let safe_time = our_time.saturating_sub(overhead);

            if let Some(mtg) = moves_to_go {
                let per_move = safe_time / mtg.max(1);
                per_move + our_inc / 2
            } else {
                let base = (safe_time / 25).max(80);
                base + our_inc * 3 / 4
            }
        }
    }

    pub mod uci {
        use super::board::{parse_fen_minimal, Position};
        use super::search::{SearchOptions, TimeManager};
        use super::types::{file_of, rank_of, Piece, Side};
        use std::io::{self, BufRead, Write};
        use std::sync::{Mutex, OnceLock};

        #[inline]
        fn pop_lsb(bb: &mut u64) -> Option<u8> {
            if *bb == 0 {
                return None;
            }
            let sq = bb.trailing_zeros() as u8;
            *bb &= *bb - 1;
            Some(sq)
        }

        fn is_square_attacked(pos: &Position, sq: u8, by: Side) -> bool {
            let by_us = by as usize;
            let occ = pos.occ_all;
            let target_bb = 1u64 << sq;

            // Pawn attacks
            let pawns = pos.pieces[by_us][Piece::Pawn as usize];
            if pawns != 0 {
                let attacks = match by {
                    Side::White => {
                        ((pawns << 7) & !0x0101_0101_0101_0101)
                            | ((pawns << 9) & !0x8080_8080_8080_8080)
                    }
                    Side::Black => {
                        ((pawns >> 9) & !0x0101_0101_0101_0101)
                            | ((pawns >> 7) & !0x8080_8080_8080_8080)
                    }
                };
                if (attacks & target_bb) != 0 {
                    return true;
                }
            }

            // Knight attacks
            let knights = pos.pieces[by_us][Piece::Knight as usize];
            if knights != 0 {
                let mut bb = knights;
                while let Some(from) = pop_lsb(&mut bb) {
                    let ff = file_of(from) as i8;
                    let fr = rank_of(from) as i8;
                    const D: [(i8, i8); 8] = [
                        (1, 2),
                        (2, 1),
                        (2, -1),
                        (1, -2),
                        (-1, -2),
                        (-2, -1),
                        (-2, 1),
                        (-1, 2),
                    ];
                    for (df, dr) in D {
                        let nf = ff + df;
                        let nr = fr + dr;
                        if (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                            let to = (nr as u8) * 8 + (nf as u8);
                            if to == sq {
                                return true;
                            }
                        }
                    }
                }
            }

            // King attacks
            let king = pos.pieces[by_us][Piece::King as usize];
            if king != 0 {
                let from = king.trailing_zeros() as u8;
                let ff = file_of(from) as i8;
                let fr = rank_of(from) as i8;
                for dr in -1..=1 {
                    for df in -1..=1 {
                        if df == 0 && dr == 0 {
                            continue;
                        }
                        let nf = ff + df;
                        let nr = fr + dr;
                        if (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                            let to = (nr as u8) * 8 + (nf as u8);
                            if to == sq {
                                return true;
                            }
                        }
                    }
                }
            }

            // Sliding attacks
            let rooks = pos.pieces[by_us][Piece::Rook as usize];
            let bishops = pos.pieces[by_us][Piece::Bishop as usize];
            let queens = pos.pieces[by_us][Piece::Queen as usize];
            let rq = rooks | queens;
            let bq = bishops | queens;

            let tf = file_of(sq) as i8;
            let tr = rank_of(sq) as i8;

            const ORTHO: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
            for (df, dr) in ORTHO {
                let mut nf = tf + df;
                let mut nr = tr + dr;
                while (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                    let nsq = (nr as u8) * 8 + (nf as u8);
                    let bb = 1u64 << nsq;
                    if (occ & bb) != 0 {
                        if (rq & bb) != 0 {
                            return true;
                        }
                        break;
                    }
                    nf += df;
                    nr += dr;
                }
            }

            const DIAG: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
            for (df, dr) in DIAG {
                let mut nf = tf + df;
                let mut nr = tr + dr;
                while (0..=7).contains(&nf) && (0..=7).contains(&nr) {
                    let nsq = (nr as u8) * 8 + (nf as u8);
                    let bb = 1u64 << nsq;
                    if (occ & bb) != 0 {
                        if (bq & bb) != 0 {
                            return true;
                        }
                        break;
                    }
                    nf += df;
                    nr += dr;
                }
            }

            false
        }

        fn parse_uci_sq(s: &str) -> Option<u8> {
            let b = s.as_bytes();
            if b.len() != 2 {
                return None;
            }
            let file = b[0];
            let rank = b[1];
            if !(b'a'..=b'h').contains(&file) || !(b'1'..=b'8').contains(&rank) {
                return None;
            }
            Some(((rank - b'1') * 8 + (file - b'a')) as u8)
        }

        fn parse_uci_move(s: &str) -> Result<(u8, u8, Option<Piece>), String> {
            let b = s.as_bytes();
            if b.len() != 4 && b.len() != 5 {
                return Err("uci move must be 4 or 5 chars".into());
            }
            let from = parse_uci_sq(&s[0..2]).ok_or("bad from-square")?;
            let to = parse_uci_sq(&s[2..4]).ok_or("bad to-square")?;
            let promo = if b.len() == 5 {
                let pc = s.chars().nth(4).unwrap();
                Some(match pc.to_ascii_lowercase() {
                    'n' => Piece::Knight,
                    'b' => Piece::Bishop,
                    'r' => Piece::Rook,
                    'q' => Piece::Queen,
                    _ => return Err("bad promotion piece".into()),
                })
            } else {
                None
            };
            Ok((from, to, promo))
        }

        #[derive(Clone)]
        struct EngineOptions {
            threads: usize,
            use_nnue: bool,
            use_tablebase: bool,
        }

        impl Default for EngineOptions {
            fn default() -> Self {
                EngineOptions {
                    threads: 1,
                    use_nnue: true,
                    use_tablebase: true,
                }
            }
        }

        static ENGINE_OPTS: OnceLock<Mutex<EngineOptions>> = OnceLock::new();

        fn get_options() -> EngineOptions {
            ENGINE_OPTS
                .get_or_init(|| Mutex::new(EngineOptions::default()))
                .lock()
                .unwrap()
                .clone()
        }

        fn set_option(name: &str, value: &str) {
            let mut opts = ENGINE_OPTS
                .get_or_init(|| Mutex::new(EngineOptions::default()))
                .lock()
                .unwrap();
            match name {
                "threads" => {
                    if let Ok(v) = value.parse::<usize>() {
                        opts.threads = v.clamp(1, 16);
                    }
                }
                "usennue" => {
                    opts.use_nnue = value.eq_ignore_ascii_case("true")
                        || value.eq_ignore_ascii_case("1")
                        || value.eq_ignore_ascii_case("yes");
                }
                "usetablebase" => {
                    opts.use_tablebase = value.eq_ignore_ascii_case("true")
                        || value.eq_ignore_ascii_case("1")
                        || value.eq_ignore_ascii_case("yes");
                }
                _ => {}
            }
        }

        fn move_to_uci(mv: super::mv::Move) -> String {
            if mv == super::mv::Move::NONE {
                return "0000".to_string();
            }
            let from = mv.from();
            let to = mv.to();
            let from_file = (b'a' + super::types::file_of(from)) as char;
            let from_rank = (b'1' + super::types::rank_of(from)) as char;
            let to_file = (b'a' + super::types::file_of(to)) as char;
            let to_rank = (b'1' + super::types::rank_of(to)) as char;

            let mut move_str = format!("{}{}{}{}", from_file, from_rank, to_file, to_rank);
            if let Some(promo) = mv.promo() {
                let promo_char = match promo {
                    super::types::Piece::Queen => 'q',
                    super::types::Piece::Rook => 'r',
                    super::types::Piece::Bishop => 'b',
                    super::types::Piece::Knight => 'n',
                    _ => 'q',
                };
                move_str.push(promo_char);
            }
            move_str
        }

        fn pv_to_uci(pv: &[super::mv::Move]) -> String {
            pv.iter()
                .map(|m| move_to_uci(*m))
                .collect::<Vec<_>>()
                .join(" ")
        }

        fn remove_any_piece_at(pos: &mut Position, sq: u8) {
            let bb = 1u64 << sq;
            for s in 0..2 {
                for p in 0..super::types::PIECE_N {
                    pos.pieces[s][p] &= !bb;
                }
            }
        }

        fn apply_uci_move_subset(pos: &mut Position, mv_str: &str) -> Result<(), String> {
            let (from, to, promo) = parse_uci_move(mv_str)?;
            let us = pos.side_to_move as usize;
            let them_side = pos.side_to_move.other();
            let them = them_side as usize;

            let from_bb = 1u64 << from;
            let to_bb = 1u64 << to;

            if (pos.occ[us] & to_bb) != 0 {
                return Err("cannot capture own piece".into());
            }

            let moving_piece = if (pos.pieces[us][Piece::Pawn as usize] & from_bb) != 0 {
                Piece::Pawn
            } else if (pos.pieces[us][Piece::Knight as usize] & from_bb) != 0 {
                Piece::Knight
            } else if (pos.pieces[us][Piece::King as usize] & from_bb) != 0 {
                Piece::King
            } else {
                return Err("subset only supports pawns/knights/kings".into());
            };

            match moving_piece {
                Piece::Pawn => {
                    let df = (file_of(to) as i8) - (file_of(from) as i8);
                    let dr = (rank_of(to) as i8) - (rank_of(from) as i8);
                    let is_capture = (pos.occ[them] & to_bb) != 0;
                    let forward = match pos.side_to_move {
                        Side::White => 1,
                        Side::Black => -1,
                    };

                    if !is_capture && df == 0 {
                        if dr == forward {
                            if (pos.occ_all & to_bb) != 0 {
                                return Err("pawn push blocked".into());
                            }
                        } else if dr == 2 * forward {
                            let start_rank = match pos.side_to_move {
                                Side::White => 1,
                                Side::Black => 6,
                            };
                            if rank_of(from) as i8 != start_rank {
                                return Err("pawn double only from start".into());
                            }
                            let mid = if forward == 1 { from + 8 } else { from - 8 };
                            if (pos.occ_all & ((1u64 << mid) | to_bb)) != 0 {
                                return Err("pawn double blocked".into());
                            }
                        } else {
                            return Err("invalid pawn push".into());
                        }
                    } else if is_capture && dr == forward && df.abs() == 1 {
                        // capture
                    } else {
                        return Err("invalid pawn move".into());
                    }

                    if (pos.occ[them] & to_bb) != 0 {
                        remove_any_piece_at(pos, to);
                    }

                    pos.pieces[us][Piece::Pawn as usize] &= !from_bb;

                    let promote_rank = match pos.side_to_move {
                        Side::White => 7,
                        Side::Black => 0,
                    };
                    if rank_of(to) == promote_rank || promo.is_some() {
                        let p = promo.unwrap_or(Piece::Queen);
                        pos.pieces[us][p as usize] |= to_bb;
                    } else {
                        pos.pieces[us][Piece::Pawn as usize] |= to_bb;
                    }
                }
                Piece::Knight => {
                    let df = (file_of(to) as i8) - (file_of(from) as i8);
                    let dr = (rank_of(to) as i8) - (rank_of(from) as i8);
                    if !((df.abs() == 1 && dr.abs() == 2) || (df.abs() == 2 && dr.abs() == 1)) {
                        return Err("invalid knight move".into());
                    }
                    if (pos.occ[them] & to_bb) != 0 {
                        remove_any_piece_at(pos, to);
                    }
                    pos.pieces[us][Piece::Knight as usize] &= !from_bb;
                    pos.pieces[us][Piece::Knight as usize] |= to_bb;
                }
                Piece::King => {
                    let df = (file_of(to) as i8) - (file_of(from) as i8);
                    let dr = (rank_of(to) as i8) - (rank_of(from) as i8);
                    let is_castle = dr == 0 && df.abs() == 2;

                    if is_castle {
                        if pos.is_square_attacked(from, them_side) {
                            return Err("cannot castle out of check".into());
                        }

                        match (pos.side_to_move, from, to) {
                            (Side::White, 4, 6) => {
                                if (pos.castling_rights & (1 << 0)) == 0 {
                                    return Err("no white O-O right".into());
                                }
                                let f1 = 5;
                                let g1 = 6;
                                if (pos.occ_all & ((1u64 << f1) | (1u64 << g1))) != 0 {
                                    return Err("castle blocked".into());
                                }
                                if (pos.pieces[us][Piece::Rook as usize] & (1u64 << 7)) == 0 {
                                    return Err("rook missing".into());
                                }
                                if pos.is_square_attacked(f1, them_side)
                                    || pos.is_square_attacked(g1, them_side)
                                {
                                    return Err("castle through check".into());
                                }
                                pos.pieces[us][Piece::King as usize] &= !(1u64 << 4);
                                pos.pieces[us][Piece::King as usize] |= 1u64 << g1;
                                pos.pieces[us][Piece::Rook as usize] &= !(1u64 << 7);
                                pos.pieces[us][Piece::Rook as usize] |= 1u64 << f1;
                                pos.castling_rights &= !((1 << 0) | (1 << 1));
                            }
                            (Side::White, 4, 2) => {
                                if (pos.castling_rights & (1 << 1)) == 0 {
                                    return Err("no white O-O-O right".into());
                                }
                                let b1 = 1;
                                let c1 = 2;
                                let d1 = 3;
                                if (pos.occ_all & ((1u64 << b1) | (1u64 << c1) | (1u64 << d1))) != 0
                                {
                                    return Err("castle blocked".into());
                                }
                                if (pos.pieces[us][Piece::Rook as usize] & (1u64 << 0)) == 0 {
                                    return Err("rook missing".into());
                                }
                                if pos.is_square_attacked(d1, them_side)
                                    || pos.is_square_attacked(c1, them_side)
                                {
                                    return Err("castle through check".into());
                                }
                                pos.pieces[us][Piece::King as usize] &= !(1u64 << 4);
                                pos.pieces[us][Piece::King as usize] |= 1u64 << c1;
                                pos.pieces[us][Piece::Rook as usize] &= !(1u64 << 0);
                                pos.pieces[us][Piece::Rook as usize] |= 1u64 << d1;
                                pos.castling_rights &= !((1 << 0) | (1 << 1));
                            }
                            (Side::Black, 60, 62) => {
                                if (pos.castling_rights & (1 << 2)) == 0 {
                                    return Err("no black O-O right".into());
                                }
                                let f8 = 61;
                                let g8 = 62;
                                if (pos.occ_all & ((1u64 << f8) | (1u64 << g8))) != 0 {
                                    return Err("castle blocked".into());
                                }
                                if (pos.pieces[us][Piece::Rook as usize] & (1u64 << 63)) == 0 {
                                    return Err("rook missing".into());
                                }
                                if pos.is_square_attacked(f8, them_side)
                                    || pos.is_square_attacked(g8, them_side)
                                {
                                    return Err("castle through check".into());
                                }
                                pos.pieces[us][Piece::King as usize] &= !(1u64 << 60);
                                pos.pieces[us][Piece::King as usize] |= 1u64 << g8;
                                pos.pieces[us][Piece::Rook as usize] &= !(1u64 << 63);
                                pos.pieces[us][Piece::Rook as usize] |= 1u64 << f8;
                                pos.castling_rights &= !((1 << 2) | (1 << 3));
                            }
                            (Side::Black, 60, 58) => {
                                if (pos.castling_rights & (1 << 3)) == 0 {
                                    return Err("no black O-O-O right".into());
                                }
                                let b8 = 57;
                                let c8 = 58;
                                let d8 = 59;
                                if (pos.occ_all & ((1u64 << b8) | (1u64 << c8) | (1u64 << d8))) != 0
                                {
                                    return Err("castle blocked".into());
                                }
                                if (pos.pieces[us][Piece::Rook as usize] & (1u64 << 56)) == 0 {
                                    return Err("rook missing".into());
                                }
                                if pos.is_square_attacked(d8, them_side)
                                    || pos.is_square_attacked(c8, them_side)
                                {
                                    return Err("castle through check".into());
                                }
                                pos.pieces[us][Piece::King as usize] &= !(1u64 << 60);
                                pos.pieces[us][Piece::King as usize] |= 1u64 << c8;
                                pos.pieces[us][Piece::Rook as usize] &= !(1u64 << 56);
                                pos.pieces[us][Piece::Rook as usize] |= 1u64 << d8;
                                pos.castling_rights &= !((1 << 2) | (1 << 3));
                            }
                            _ => return Err("invalid castle".into()),
                        }
                    } else {
                        if df.abs() > 1 || dr.abs() > 1 {
                            return Err("invalid king move".into());
                        }
                        if (pos.occ[them] & to_bb) != 0 {
                            remove_any_piece_at(pos, to);
                        }
                        pos.pieces[us][Piece::King as usize] &= !from_bb;
                        pos.pieces[us][Piece::King as usize] |= to_bb;
                        match pos.side_to_move {
                            Side::White => pos.castling_rights &= !((1 << 0) | (1 << 1)),
                            Side::Black => pos.castling_rights &= !((1 << 2) | (1 << 3)),
                        }
                    }
                }
                _ => return Err("unreachable".into()),
            }

            pos.side_to_move = pos.side_to_move.other();
            pos.recompute_occ();
            Ok(())
        }

        fn parse_position(cmd: &str) -> Result<(Position, usize), String> {
            let toks: Vec<&str> = cmd.split_whitespace().collect();
            if toks.is_empty() || toks[0] != "position" {
                return Err("not a position command".into());
            }
            if toks.len() < 2 {
                return Err("position missing arguments".into());
            }

            if toks[1] == "startpos" {
                return Ok((Position::startpos(), 2));
            }

            if toks[1] == "fen" {
                if toks.len() < 2 + 1 + 6 {
                    return Err("position fen missing fen fields".into());
                }
                let fen6 = toks[2..8].join(" ");
                let b = parse_fen_minimal(&fen6)?;
                return Ok((b, 8));
            }

            Err("position expects startpos or fen".into())
        }

        pub fn loop_uci(mut pos: Position) -> io::Result<()> {
            let stdin = io::stdin();
            let mut out = io::stdout();

            for line in stdin.lock().lines() {
                let line = line?;
                let cmd = line.trim();

                match cmd {
                    "uci" => {
                        writeln!(out, "id name VDMO (Rust)")?;
                        writeln!(out, "id author you")?;
                        writeln!(
                            out,
                            "option name Threads type spin default 1 min 1 max 16"
                        )?;
                        writeln!(
                            out,
                            "option name UseNNUE type check default true"
                        )?;
                        writeln!(
                            out,
                            "option name UseTablebase type check default true"
                        )?;
                        writeln!(out, "uciok")?;
                    }
                    "isready" => {
                        writeln!(out, "readyok")?;
                    }
                    "ucinewgame" => {
                        pos = Position::startpos();
                        writeln!(out, "info string newgame")?;
                    }
                    "quit" => break,
                    _ if cmd.starts_with("position") => match parse_position(cmd) {
                        Ok((mut b, idx)) => {
                            let toks: Vec<&str> = cmd.split_whitespace().collect();
                            if idx < toks.len() && toks[idx] == "moves" {
                                let mut applied = 0usize;
                                let mut rejected = 0usize;
                                for &m in toks.iter().skip(idx + 1) {
                                    match apply_uci_move_subset(&mut b, m) {
                                        Ok(()) => applied += 1,
                                        Err(_) => rejected += 1,
                                    }
                                }
                                pos = b;
                                writeln!(
                                        out,
                                        "info string position ok (applied {applied}, rejected {rejected})"
                                    )?;
                            } else {
                                pos = b;
                                writeln!(out, "info string position ok")?;
                            }
                        }
                        Err(e) => {
                            writeln!(out, "info string position error: {e}")?;
                        }
                    },
                    _ if cmd.starts_with("setoption") => {
                        let tokens: Vec<&str> = cmd.split_whitespace().collect();
                        let mut name = None;
                        let mut value = None;
                        let mut i = 1;
                        while i < tokens.len() {
                            match tokens[i] {
                                "name" => {
                                    if i + 1 < tokens.len() {
                                        name = Some(tokens[i + 1].to_ascii_lowercase());
                                        i += 2;
                                    } else {
                                        i += 1;
                                    }
                                }
                                "value" => {
                                    if i + 1 < tokens.len() {
                                        value = Some(tokens[i + 1].to_ascii_lowercase());
                                        i += 2;
                                    } else {
                                        i += 1;
                                    }
                                }
                                _ => i += 1,
                            }
                        }
                        if let (Some(n), Some(v)) = (name, value) {
                            set_option(&n, &v);
                        }
                    }
                    _ if cmd.starts_with("go") => {
                        // Parse go command for time controls
                        let tokens: Vec<&str> = cmd.split_whitespace().collect();
                        let mut depth = 10; // Default max depth
                        let mut wtime = None;
                        let mut btime = None;
                        let mut winc = 0u64;
                        let mut binc = 0u64;
                        let mut movestogo = None;
                        let mut movetime = None;
                        let mut infinite = false;

                        let mut i = 1;
                        while i < tokens.len() {
                            match tokens[i] {
                                "depth" => {
                                    if i + 1 < tokens.len() {
                                        depth = tokens[i + 1].parse().unwrap_or(10);
                                        i += 2;
                                    } else {
                                        i += 1;
                                    }
                                }
                                "wtime" => {
                                    if i + 1 < tokens.len() {
                                        wtime = tokens[i + 1].parse().ok();
                                        i += 2;
                                    } else {
                                        i += 1;
                                    }
                                }
                                "btime" => {
                                    if i + 1 < tokens.len() {
                                        btime = tokens[i + 1].parse().ok();
                                        i += 2;
                                    } else {
                                        i += 1;
                                    }
                                }
                                "winc" => {
                                    if i + 1 < tokens.len() {
                                        winc = tokens[i + 1].parse().unwrap_or(0);
                                        i += 2;
                                    } else {
                                        i += 1;
                                    }
                                }
                                "binc" => {
                                    if i + 1 < tokens.len() {
                                        binc = tokens[i + 1].parse().unwrap_or(0);
                                        i += 2;
                                    } else {
                                        i += 1;
                                    }
                                }
                                "movestogo" => {
                                    if i + 1 < tokens.len() {
                                        movestogo = tokens[i + 1].parse().ok();
                                        i += 2;
                                    } else {
                                        i += 1;
                                    }
                                }
                                "movetime" => {
                                    if i + 1 < tokens.len() {
                                        movetime = tokens[i + 1].parse().ok();
                                        i += 2;
                                    } else {
                                        i += 1;
                                    }
                                }
                                "infinite" => {
                                    infinite = true;
                                    i += 1;
                                }
                                _ => {
                                    i += 1;
                                }
                            }
                        }

                        // Calculate time allocation
                        let time_manager = if infinite {
                            TimeManager::infinite()
                        } else if let Some(mt) = movetime {
                            TimeManager::new(mt)
                        } else {
                            let our_time = match pos.side_to_move {
                                Side::White => wtime.unwrap_or(60000),
                                Side::Black => btime.unwrap_or(60000),
                            };
                            let our_inc = match pos.side_to_move {
                                Side::White => winc,
                                Side::Black => binc,
                            };
                            let allocated =
                                super::search::calculate_time(our_time, our_inc, movestogo);
                            let overhead = 30u64;
                            let safe_time = our_time.saturating_sub(overhead);
                            let soft = allocated.min(safe_time);
                            let hard = (soft * 3).min(safe_time.max(soft));
                            TimeManager::with_limits(soft, hard)
                        };

                        let opts = get_options();
                        let search_opts = SearchOptions {
                            use_nnue: opts.use_nnue,
                            use_tablebase: opts.use_tablebase,
                            threads: opts.threads,
                        };

                        let report = super::search::search_position(
                            &mut pos,
                            depth as u8,
                            &time_manager,
                            search_opts,
                        );

                        for info in report.infos {
                            let pv = pv_to_uci(&info.pv);
                            writeln!(
                                out,
                                "info depth {} score cp {} nodes {} time {} pv {}",
                                info.depth, info.score, info.nodes, info.time_ms, pv
                            )?;
                        }

                        if report.best_move == super::mv::Move::NONE {
                            writeln!(out, "bestmove 0000")?;
                        } else {
                            let move_str = move_to_uci(report.best_move);
                            writeln!(out, "bestmove {}", move_str)?;
                        }
                    }
                    _ => {}
                }

                out.flush()?;
            }

            Ok(())
        }
    }
}

pub use core::board::{parse_fen_minimal, Position};
pub use core::mv::Move;
pub use core::types::{Bitboard, MoveType, Piece, Side, PIECE_N};
