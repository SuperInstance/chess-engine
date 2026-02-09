//! VDMO engine binary.
//!
//! This binary is intentionally thin: it wires CLI + UCI I/O to the engine
//! implementation living in the `vdmo` library crate.

use vdmo::{parse_fen_minimal, Position};

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "uci".to_string());

    match cmd.as_str() {
        "uci" => {
            let pos = Position::startpos();
            let _ = vdmo::core::uci::loop_uci(pos);
        }
        "perft" => {
            let depth: u8 = args
                .next()
                .unwrap_or_else(|| "1".to_string())
                .parse()
                .unwrap_or(1);

            let fen_rest: Vec<String> = args.collect();
            let mut pos = if fen_rest.is_empty() {
                Position::startpos()
            } else {
                let fen = fen_rest.join(" ");
                match parse_fen_minimal(&fen) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("FEN error: {e}");
                        std::process::exit(2);
                    }
                }
            };

            // Perft now works with real movegen + make/unmake
            let nodes = vdmo::core::perft::perft(&mut pos, depth);
            println!("{nodes}");
        }
        _ => {
            eprintln!("usage:");
            eprintln!("  vdmo uci");
            eprintln!("  vdmo perft <depth> [fen]");
            std::process::exit(2);
        }
    }
}
