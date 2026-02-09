//! Minimal VDMO chess GUI using eframe/egui.
//!
//! Goals:
//! - Open a native window (so double-clicking shows something).
//! - Render an 8x8 chessboard with simple piece glyphs.
//! - Support basic click-to-move on the GUI board state.
//!
//! Notes:
//! - This does NOT validate chess legality.
//! - It does NOT talk to the engine yet. It’s a UI scaffold.
//! - Next step after this: integrate with the engine's board + move parsing,
//!   then generate/apply legal moves and/or talk UCI internally.

use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("VDMO Chess GUI")
            .with_inner_size([720.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "VDMO Chess GUI",
        native_options,
        Box::new(|cc| Box::new(VdmoChessGuiApp::new(cc))),
    )
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Side {
    White,
    Black,
}

impl Side {
    fn other(self) -> Side {
        match self {
            Side::White => Side::Black,
            Side::Black => Side::White,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Piece {
    side: Side,
    kind: PieceKind,
}

#[derive(Clone)]
struct UiBoard {
    squares: [Option<Piece>; 64], // 0=a1 .. 63=h8
}

impl UiBoard {
    fn empty() -> Self {
        Self {
            squares: [None; 64],
        }
    }

    fn startpos() -> Self {
        let mut b = Self::empty();

        // Pawns
        for file in 0..8 {
            b.set(
                file,
                1,
                Some(Piece {
                    side: Side::White,
                    kind: PieceKind::Pawn,
                }),
            );
            b.set(
                file,
                6,
                Some(Piece {
                    side: Side::Black,
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
        for (file, kind) in back.into_iter().enumerate() {
            b.set(
                file as i32,
                0,
                Some(Piece {
                    side: Side::White,
                    kind,
                }),
            );
            b.set(
                file as i32,
                7,
                Some(Piece {
                    side: Side::Black,
                    kind,
                }),
            );
        }

        b
    }

    #[inline]
    fn idx(file: i32, rank: i32) -> Option<usize> {
        if (0..=7).contains(&file) && (0..=7).contains(&rank) {
            Some((rank as usize) * 8 + (file as usize))
        } else {
            None
        }
    }

    #[inline]
    fn get(&self, file: i32, rank: i32) -> Option<Piece> {
        Self::idx(file, rank).and_then(|i| self.squares[i])
    }

    #[inline]
    fn set(&mut self, file: i32, rank: i32, p: Option<Piece>) {
        if let Some(i) = Self::idx(file, rank) {
            self.squares[i] = p;
        }
    }

    #[inline]
    fn get_sq(&self, sq: usize) -> Option<Piece> {
        self.squares[sq]
    }

    #[inline]
    fn set_sq(&mut self, sq: usize, p: Option<Piece>) {
        self.squares[sq] = p;
    }

    fn move_piece(&mut self, from: usize, to: usize) -> Result<(), &'static str> {
        if from >= 64 || to >= 64 {
            return Err("square out of range");
        }
        let p = self.squares[from].ok_or("no piece on from-square")?;
        self.squares[from] = None;
        self.squares[to] = Some(p);
        Ok(())
    }

    fn try_castle(&mut self, from: usize, to: usize) -> Result<bool, &'static str> {
        let king = self.squares[from].ok_or("no piece on from-square")?;
        if king.kind != PieceKind::King {
            return Ok(false);
        }
        let (ff, fr) = sq_to_coord(from);
        let (tf, tr) = sq_to_coord(to);
        if fr != tr {
            return Ok(false);
        }
        let file_delta = tf - ff;
        if file_delta.abs() != 2 {
            return Ok(false);
        }

        let (rook_from_file, rook_to_file) = if file_delta > 0 {
            (7, ff + 1)
        } else {
            (0, ff - 1)
        };
        let rook_from = coord_to_sq(rook_from_file, fr).ok_or("rook square invalid")?;
        let rook_to = coord_to_sq(rook_to_file, fr).ok_or("rook target invalid")?;
        let rook = self.squares[rook_from].ok_or("rook missing")?;
        if rook.kind != PieceKind::Rook || rook.side != king.side {
            return Err("rook missing for castling");
        }

        self.squares[from] = None;
        self.squares[to] = Some(king);
        self.squares[rook_from] = None;
        self.squares[rook_to] = Some(rook);
        Ok(true)
    }
}

fn piece_glyph(p: Piece) -> &'static str {
    // Unicode chess symbols (widely supported). If you prefer letters, swap these.
    match (p.side, p.kind) {
        (Side::White, PieceKind::King) => "♔",
        (Side::White, PieceKind::Queen) => "♕",
        (Side::White, PieceKind::Rook) => "♖",
        (Side::White, PieceKind::Bishop) => "♗",
        (Side::White, PieceKind::Knight) => "♘",
        (Side::White, PieceKind::Pawn) => "♙",
        (Side::Black, PieceKind::King) => "♚",
        (Side::Black, PieceKind::Queen) => "♛",
        (Side::Black, PieceKind::Rook) => "♜",
        (Side::Black, PieceKind::Bishop) => "♝",
        (Side::Black, PieceKind::Knight) => "♞",
        (Side::Black, PieceKind::Pawn) => "♟",
    }
}

fn sq_to_coord(sq: usize) -> (i32, i32) {
    let file = (sq % 8) as i32;
    let rank = (sq / 8) as i32;
    (file, rank)
}

fn coord_to_sq(file: i32, rank: i32) -> Option<usize> {
    if (0..=7).contains(&file) && (0..=7).contains(&rank) {
        Some((rank as usize) * 8 + (file as usize))
    } else {
        None
    }
}

fn sq_name(file: i32, rank: i32) -> String {
    let f = (b'a' + file as u8) as char;
    let r = (b'1' + rank as u8) as char;
    format!("{f}{r}")
}

struct VdmoChessGuiApp {
    board: UiBoard,
    side_to_move: Side,

    // UI state
    selected: Option<usize>,
    status: String,
    flip_board: bool,
}

impl VdmoChessGuiApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            board: UiBoard::startpos(),
            side_to_move: Side::White,
            selected: None,
            status: "Click a piece, then click a destination square.".to_string(),
            flip_board: false,
        }
    }

    fn reset(&mut self) {
        self.board = UiBoard::startpos();
        self.side_to_move = Side::White;
        self.selected = None;
        self.status = "Reset to start position.".to_string();
    }

    fn try_click_square(&mut self, sq: usize) {
        let Some(piece) = self.board.get_sq(sq) else {
            // Empty square
            if let Some(from) = self.selected {
                if from == sq {
                    self.selected = None;
                    self.status = "Selection cleared.".to_string();
                    return;
                }
                match self.board.try_castle(from, sq).and_then(|did_castle| {
                    if did_castle {
                        Ok(())
                    } else {
                        self.board.move_piece(from, sq)
                    }
                }) {
                    Ok(()) => {
                        let (ff, fr) = sq_to_coord(from);
                        let (tf, tr) = sq_to_coord(sq);
                        self.selected = None;
                        self.side_to_move = self.side_to_move.other();
                        self.status = format!(
                            "Moved {} -> {}. (No legality checking)",
                            sq_name(ff, fr),
                            sq_name(tf, tr)
                        );
                    }
                    Err(e) => {
                        self.status = format!("Move failed: {e}");
                    }
                }
            } else {
                self.status = "No piece selected.".to_string();
            }
            return;
        };

        // Occupied square
        if let Some(from) = self.selected {
            if from == sq {
                self.selected = None;
                self.status = "Selection cleared.".to_string();
                return;
            }

            // If clicked a friendly piece, just re-select it (common UX)
            let from_piece = self.board.get_sq(from);
            if let Some(fp) = from_piece {
                if fp.side == piece.side {
                    self.selected = Some(sq);
                    self.status = format!("Selected {}.", piece_glyph(piece));
                    return;
                }
            }

            // Otherwise treat as capture destination (still no legality)
            match self.board.try_castle(from, sq).and_then(|did_castle| {
                if did_castle {
                    Ok(())
                } else {
                    self.board.move_piece(from, sq)
                }
            }) {
                Ok(()) => {
                    let (ff, fr) = sq_to_coord(from);
                    let (tf, tr) = sq_to_coord(sq);
                    self.selected = None;
                    self.side_to_move = self.side_to_move.other();
                    self.status = format!(
                        "Moved {} -> {} (capture if occupied). (No legality checking)",
                        sq_name(ff, fr),
                        sq_name(tf, tr)
                    );
                }
                Err(e) => self.status = format!("Move failed: {e}"),
            }
        } else {
            // Only allow selecting side-to-move pieces (basic UX)
            if piece.side != self.side_to_move {
                self.status = "It's not that side's turn (UI only).".to_string();
                return;
            }
            self.selected = Some(sq);
            self.status = format!("Selected {}.", piece_glyph(piece));
        }
    }

    fn draw_board(&mut self, ui: &mut egui::Ui) {
        // Board in an egui::Grid so we can get per-square click responses.
        // Rank 7 at top if not flipped.
        let light = egui::Color32::from_rgb(240, 217, 181);
        let dark = egui::Color32::from_rgb(181, 136, 99);
        let sel = egui::Color32::from_rgb(120, 170, 255);

        let square_size = egui::Vec2::splat(56.0);
        let font_size = 32.0;

        let ranks: Vec<i32> = if self.flip_board {
            (0..=7).collect()
        } else {
            (0..=7).rev().collect()
        };
        let files: Vec<i32> = if self.flip_board {
            (0..=7).rev().collect()
        } else {
            (0..=7).collect()
        };

        egui::Grid::new("vdmo_chess_board_grid")
            .spacing(egui::vec2(0.0, 0.0))
            .show(ui, |ui| {
                for &rank in &ranks {
                    for &file in &files {
                        let sq = coord_to_sq(file, rank).unwrap();
                        let is_light = ((file + rank) & 1) == 0;

                        let mut fill = if is_light { light } else { dark };
                        if self.selected == Some(sq) {
                            fill = sel;
                        }

                        let piece = self.board.get(file, rank);
                        let text = piece.map(piece_glyph).unwrap_or(" ");

                        let response = ui.add_sized(
                            square_size,
                            egui::Button::new(
                                egui::RichText::new(text)
                                    .size(font_size)
                                    .color(egui::Color32::BLACK),
                            )
                            .fill(fill)
                            .frame(true),
                        );

                        if response.clicked() {
                            self.try_click_square(sq);
                        }
                    }
                    ui.end_row();
                }
            });
    }
}

impl eframe::App for VdmoChessGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Reset").clicked() {
                    self.reset();
                }
                ui.checkbox(&mut self.flip_board, "Flip board");
                ui.separator();
                ui.label(format!(
                    "Side to move: {}",
                    match self.side_to_move {
                        Side::White => "White",
                        Side::Black => "Black",
                    }
                ));
            });
        });

        egui::SidePanel::right("right_panel")
            .resizable(false)
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.heading("VDMO");
                ui.label("Minimal GUI scaffold");
                ui.separator();

                ui.label("Status:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.status)
                        .desired_rows(8)
                        .interactive(false),
                );

                ui.separator();
                ui.label("Notes:");
                ui.label("- No full legality checking yet.");
                ui.label("- Castling supported via king move.");
                ui.label("- No engine integration yet.");
                ui.label("- Next: connect to engine state + movegen.");
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                self.draw_board(ui);
            });
        });
    }
}
