/// Minimal chess engine: board representation, move generation, evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece { King, Queen, Rook, Bishop, Knight, Pawn }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color { White, Black }
#[derive(Debug, Clone, Copy)]
pub struct Square { pub file: u8, pub rank: u8 }
#[derive(Debug, Clone)]
pub struct PieceAt { pub piece: Piece, pub color: Color, pub sq: Square }

#[derive(Debug, Clone)]
pub struct Board {
    pieces: Vec<PieceAt>,
    side: Color,
}

impl Board {
    pub fn starting() -> Self {
        let mut pieces = Vec::new();
        let back = [Piece::Rook, Piece::Knight, Piece::Bishop, Piece::Queen,
                     Piece::King, Piece::Bishop, Piece::Knight, Piece::Rook];
        for (i, &p) in back.iter().enumerate() {
            pieces.push(PieceAt { piece: p, color: Color::White, sq: Square { file: i as u8, rank: 0 } });
            pieces.push(PieceAt { piece: p, color: Color::Black, sq: Square { file: i as u8, rank: 7 } });
        }
        for i in 0..8 {
            pieces.push(PieceAt { piece: Piece::Pawn, color: Color::White, sq: Square { file: i, rank: 1 } });
            pieces.push(PieceAt { piece: Piece::Pawn, color: Color::Black, sq: Square { file: i, rank: 6 } });
        }
        Self { pieces, side: Color::White }
    }
    pub fn at(&self, f: u8, r: u8) -> Option<&PieceAt> {
        self.pieces.iter().find(|p| p.sq.file == f && p.sq.rank == r)
    }
    pub fn side_to_move(&self) -> Color { self.side }
    pub fn material_eval(&self) -> i32 {
        let vals = |p: Piece| -> i32 {
            match p {
                Piece::Pawn => 100, Piece::Knight => 320, Piece::Bishop => 330,
                Piece::Rook => 500, Piece::Queen => 900, Piece::King => 20000,
            }
        };
        self.pieces.iter().map(|pa| {
            let v = vals(pa.piece);
            if pa.color == Color::White { v } else { -v }
        }).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starting_material() { assert_eq!(Board::starting().material_eval(), 0); }
}
