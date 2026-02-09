# VDMO Chess Engine

A chess engine written in Rust with a focus on correctness and clarity.

## Features

### ✅ Implemented

- **Bitboard representation** for efficient board state
- **Full move generation** for all pieces (pawns, knights, bishops, rooks, queens, king)
- **Special moves**: castling (king-side and queen-side), en-passant, promotions
- **Make/unmake** with state stack for efficient search
- **Attack detection** for all piece types (used in legality checking)
- **Perft** (performance test) for move generation validation
- **Alpha-beta search** with:
  - Iterative deepening
  - Quiescence search (tactical extensions)
  - Mate detection
  - **Late Move Reductions (LMR)** for efficient deep search
  - **Null-Move Pruning** for forward pruning
- **Transposition table** with Zobrist hashing (1M entries)
- **Advanced move ordering**:
  - TT move (transposition table best move first)
  - MVV-LVA (Most Valuable Victim - Least Valuable Attacker) for captures
  - Killer moves (2 killers per ply)
  - History heuristic (move success tracking)
- **Enhanced evaluation**:
  - Material counting
  - Piece-square tables for all pieces
  - Positional understanding
- **Time management**:
  - Classical time controls (wtime/btime/winc/binc)
  - Move time controls (movetime)
  - Moves-to-go support
  - Adaptive time allocation
- **UCI protocol** support for GUI integration
- **Native GUI** (`vdmo-chess-gui.exe`) using egui/eframe

### 🚧 To Be Implemented

- Counter moves and follow-up moves
- More evaluation features (pawn structure, king safety, mobility)
- Aspiration windows
- Principal variation (PV) extraction and display
- Opening book
- Endgame tablebases
- NNUE evaluation (long-term goal)

## Building

### Requirements

- Rust 1.70+ ([install here](https://rustup.rs/))

### Build Commands

```bash
# Build release (optimized)
cargo build --release

# Build both binaries
cargo build --release --bin vdmo            # Engine
cargo build --release --bin vdmo-chess-gui  # GUI
```

Binaries will be in `target/release/`:
- `vdmo.exe` - UCI engine
- `vdmo-chess-gui.exe` - Graphical interface

## Usage

### 1. Perft (Move Generation Testing)

Validate move generation correctness:

```bash
# Test from starting position
cargo run --release --bin vdmo -- perft 5

# Test from custom FEN
cargo run --release --bin vdmo -- perft 4 "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
```

**Expected perft values from startpos:**
- Depth 1: 20
- Depth 2: 400
- Depth 3: 8,902
- Depth 4: 197,281
- Depth 5: 4,865,609

### 2. UCI Mode (Chess GUIs)

Run as a UCI engine:

```bash
cargo run --release --bin vdmo -- uci
```

Then interact via stdin:
```
uci
isready
position startpos moves e2e4 e7e5
go depth 6
# or with time controls:
go wtime 300000 btime 300000 winc 2000 binc 2000
# or with fixed time per move:
go movetime 5000
quit
```

### 3. Use with Chess GUIs

Load `vdmo.exe` into any UCI-compatible GUI:

- **Arena Chess** (Windows)
- **BanksiaGUI** (Cross-platform)
- **CuteChess** (Cross-platform)
- **lichess-bot** (online play)

Configuration:
- Protocol: **UCI**
- Executable: `target/release/vdmo.exe`

### 4. Built-in GUI

Run the native graphical interface:

```bash
cargo run --release --bin vdmo-chess-gui
```

Features:
- Visual chessboard with piece rendering
- Click-to-move interaction
- Board flip option
- Reset to start position

**Note:** GUI currently uses independent board state. Engine integration is planned.

## Architecture

```
vdmo/
├── src/
│   ├── lib.rs              # Engine library
│   │   ├── core::types     # Basic types (Side, Piece, Bitboard)
│   │   ├── core::mv        # Move representation (packed 16-bit)
│   │   ├── core::board     # Position state, make/unmake
│   │   ├── core::movegen   # Pseudo-legal move generation
│   │   ├── core::search    # Alpha-beta, quiescence, eval
│   │   ├── core::perft     # Move generation validator
│   │   └── core::uci       # UCI protocol handler
│   ├── main.rs             # UCI/CLI binary
│   └── bin/
│       └── vdmo-chess-gui.rs  # eframe/egui GUI binary
└── Cargo.toml
```

### Key Design Decisions

**Bitboards over arrays:** All piece positions stored as `u64` bitmasks for fast move generation.

**Make/unmake over copy-make:** Position state is modified in-place with undo stack for efficiency.

**Ray-based sliding moves:** No magic bitboards yet—simple ray-walking for correctness-first development.

**Separate binaries:** Engine (`vdmo`) and GUI (`vdmo-chess-gui`) share the library crate.

## Development

### Run Tests

```bash
cargo test
```

### Check for Issues

```bash
cargo clippy
```

### Format Code

```bash
cargo fmt
```

### Perft Validation Suite

```bash
# Run against known positions
cargo run --release --bin vdmo -- perft 5  # startpos
cargo run --release --bin vdmo -- perft 5 "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"  # kiwipete
```

## Current Strength

With latest improvements:

- **Estimated ELO:** ~2000-2200 (advanced intermediate)
- **Search depth:** 10-12 ply in reasonable time (with LMR and null-move pruning)
- **Evaluation:** Material + piece-square tables for positional understanding
- **Features**: 
  - Transposition table with Zobrist hashing
  - Advanced move ordering (TT move, MVV-LVA, killers, history)
  - Quiescence search
  - Time management
  - Iterative deepening

Strength will improve further with:
1. Better evaluation (pawn structure, king safety) (~200-300 ELO gain)
2. Aspiration windows (~20-30 ELO gain)
3. Magic bitboards (~50-100 ELO via speed gain)
4. Multi-threading (~100-200 ELO gain)

## Contributing

This is a learning project focused on chess engine development fundamentals.

Key areas for improvement:
- [ ] Magic bitboards for faster move generation
- [x] Transposition table with Zobrist hashing ✅
- [x] MVV-LVA move ordering for captures ✅
- [x] Piece-square tables for evaluation ✅
- [x] Killer moves and history heuristic ✅
- [x] UCI time management ✅
- [x] Null-move pruning ✅
- [x] Late move reductions (LMR) ✅
- [ ] Aspiration windows
- [ ] Opening book support

## Resources

**Chess Programming:**
- [Chess Programming Wiki](https://www.chessprogramming.org/)
- [Perft Results](https://www.chessprogramming.org/Perft_Results)
- [UCI Protocol](https://www.chessprogramming.org/UCI)

**Similar Projects:**
- [Stockfish](https://github.com/official-stockfish/Stockfish) (C++, strongest engine)
- [Viridithas](https://github.com/cosmobobak/viridithas) (Rust, strong)
- [cozy-chess](https://github.com/analog-hors/cozy-chess) (Rust, move generation library)

## License

MIT

## Author

Built following best practices for chess engine development in Rust.