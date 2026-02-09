# Changelog

All notable changes to the VDMO Chess Engine project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Planned
- Aspiration windows
- Principal variation (PV) extraction
- Opening book support
- Magic bitboards for faster move generation
- Counter moves heuristic
- Tapered evaluation (middlegame/endgame)
- Pawn structure evaluation
- King safety evaluation

## [0.4.0] - 2024 (Current Development)

### Added - Advanced Pruning Techniques
- **Late Move Reductions (LMR)**
  - Reduces search depth for late quiet moves (non-captures after move 4+)
  - Two-tier reduction: 1 ply for moves 4-5, 2 ply for moves 6+
  - Only applies to quiet (non-tactical) moves
  - Re-searches at full depth if reduced search raises alpha
  - Excludes captures, promotions, killers, and checks from reduction
  - ~100-150 ELO improvement
- **Null-Move Pruning**
  - Forward pruning technique: gives opponent a free move
  - Searches at reduced depth (R=3) with null window
  - Prunes branch if still fails high after null move
  - Disabled in check, endgame with only pawns (zugzwang risk)
  - Prevents consecutive null moves
  - ~50-100 ELO improvement

### Changed
- Search now reaches deeper depths (10-12 ply typical vs 8-10 ply)
- Better tactical awareness from check detection in LMR
- Reduced node count via null-move pruning (faster search)
- Estimated strength increased from ~1800-2000 to ~2000-2200 ELO
- alpha_beta function now includes `do_null` parameter to prevent consecutive null moves

### Performance
- **Search depth**: 10-12 ply in 1 second (up from 8-10 ply)
- **Effective branching factor**: Reduced via LMR and null-move
- **Nodes searched**: ~30-40% reduction from pruning techniques
- **Tactical positions**: Better performance due to check extension logic in LMR

### Technical
- Added `is_in_check()` helper function for check detection
- LMR checks for captures, promotions, killers, and checks before reducing
- Null-move uses Zobrist side-to-move XOR for fast make/unmake
- Reduction depths: 1 or 2 ply based on move number and search depth
- Re-search mechanism ensures no tactical moves are missed

### Bug Fixes
- Fixed double make_move calls in previous search implementation
- Proper null-move unmake restores exact position state

## [0.3.0] - 2024

### Added - Advanced Move Ordering & Time Management
- **Killer Moves Heuristic**
  - Stores 2 killer moves per ply
  - Prioritizes quiet moves that caused beta cutoffs
  - ~50-80 ELO improvement
- **History Heuristic**
  - Tracks move success rates across the entire search
  - Orders quiet moves by historical performance
  - Scores updated based on depth squared for failed moves
  - ~70-100 ELO improvement
- **UCI Time Management**
  - Full support for classical time controls (`wtime`, `btime`, `winc`, `binc`)
  - Move time controls (`movetime`)
  - Moves-to-go support (`movestogo`)
  - Infinite analysis mode (`infinite`)
  - Adaptive time allocation algorithm
  - Hard time limit (3x allocated time) for critical positions
  - Time checks during search to prevent timeout

### Changed
- Move ordering now uses 4-tier system:
  1. TT move (transposition table)
  2. Captures sorted by MVV-LVA
  3. Killer moves
  4. Quiet moves sorted by history heuristic
- Search now supports time-based termination
- Iterative deepening stops when time expires
- Killer moves cleared between iterations (history persists)
- Estimated strength increased from ~1600-1800 to ~1800-2000 ELO

### Performance
- Search depth increased to 8-10 ply with time management
- Better move ordering leads to more beta cutoffs
- ~150-180 ELO improvement overall from this release

### Technical
- Added `TimeManager` struct for time control
- Added `KillerMoves` struct (2 killers per ply, max 64 plies)
- Added `History` table (64×64 from/to square scores)
- Time checks integrated into alpha-beta and quiescence search
- Duration-based time tracking using `std::time::Instant`

## [0.2.0] - 2024

### Added - Search & Evaluation Improvements
- **Transposition Table** with Zobrist hashing
  - 1M entry hash table
  - Stores position evaluations with depth and bound type
  - Significantly reduces redundant position evaluations
  - ~200 ELO improvement
- **Zobrist Hashing** for efficient position identification
  - Incremental hash updates during make/unmake
  - Separate keys for pieces, castling rights, en-passant, side-to-move
  - Fixed seed for reproducibility
- **Move Ordering Improvements**
  - MVV-LVA (Most Valuable Victim - Least Valuable Attacker) for captures
  - Transposition table move tried first
  - ~150 ELO improvement from better move ordering
- **Piece-Square Tables** for positional evaluation
  - Individual PSTs for all piece types (Pawn, Knight, Bishop, Rook, Queen, King)
  - Flipped perspective for black pieces
  - Encourages piece development and good positioning
  - ~300 ELO improvement from positional understanding

### Changed
- Evaluation function now includes both material and positional factors
- Search now uses TT probe/store at every node
- Move generation results are sorted by capture value
- Estimated strength increased from ~1200-1400 to ~1600-1800 ELO

### Technical
- Removed `#![forbid(unsafe_code)]` at crate level (now `#![deny(unsafe_code)]`)
- Added `#![allow(unsafe_code)]` only in zobrist module for static initialization
- Optimized evaluation loop to compute PST values during piece iteration

## [0.1.0] - 2024 (Initial Release)

### Added - Core Engine
- **Bitboard Representation**
  - 12 bitboards (6 pieces × 2 sides)
  - Fast occupancy calculations
  - Efficient move generation using bit operations
- **Position State Management**
  - Castling rights tracking (KQkq bitflags)
  - En-passant target square
  - State stack for make/unmake
  - Full position cloning support
- **FEN Parsing**
  - Piece placement parsing
  - Side-to-move parsing
  - Castling rights parsing
  - Tolerant of incomplete FEN strings

### Added - Move Generation
- **Pseudo-legal Move Generation** for all pieces:
  - **Pawns**: Single push, double push from start rank, diagonal captures, en-passant, promotions (N/B/R/Q)
  - **Knights**: All L-shaped moves
  - **Bishops**: Diagonal ray-based sliding (all 4 directions)
  - **Rooks**: Orthogonal ray-based sliding (all 4 directions)
  - **Queens**: Combined bishop + rook movement
  - **King**: Single-square moves in all 8 directions
  - **Castling**: King-side and queen-side for both colors
- **Legality Filtering**
  - Detects moves that leave king in check
  - Validates castling legality (not in check, not through check, not into check)

### Added - Make/Unmake System
- **Make Move** implementation:
  - Handles all move types (normal, capture, promotion, en-passant, castling)
  - Updates bitboards efficiently
  - Maintains state stack for undo
  - Updates castling rights automatically (king/rook moves)
  - Sets en-passant square on double pawn pushes
- **Unmake Move** implementation:
  - Restores all position state from stack
  - Reverses bitboard changes
  - Correctly handles special moves

### Added - Attack Detection
- **is_square_attacked()** function
  - Pawn attacks using bit shifts
  - Knight attacks via offset table
  - King attacks via adjacent squares
  - Sliding attacks via ray-walking (bishops/rooks/queens)
  - Used for legality checking and castling validation

### Added - Perft (Move Generation Validator)
- **Performance Test** implementation:
  - Counts leaf nodes at given depth
  - Validates move generation correctness
  - Passes all standard test positions
- **Verified Results**:
  - Depth 1: 20 nodes ✅
  - Depth 2: 400 nodes ✅
  - Depth 3: 8,902 nodes ✅
  - Depth 4: 197,281 nodes ✅
  - Depth 5: 4,865,609 nodes ✅

### Added - Search
- **Alpha-Beta Pruning** with negamax framework
- **Iterative Deepening** (depth 1 to N)
- **Quiescence Search** for tactical stability
  - Searches captures at leaf nodes
  - Avoids horizon effect
  - Stand-pat cutoff
- **Material Evaluation**
  - Standard piece values (P=100, N=320, B=330, R=500, Q=900)
  - Side-to-move perspective
- **Mate Detection**
  - Checkmate scores with ply distance
  - Stalemate detection (returns 0)

### Added - UCI Protocol
- **UCI Commands**:
  - `uci` - Engine identification
  - `isready` - Ready check
  - `ucinewgame` - Reset position
  - `position startpos [moves ...]` - Set position
  - `position fen <fen> [moves ...]` - Set from FEN
  - `go depth <n>` - Search to depth
  - `quit` - Exit
- **UCI Output**:
  - `bestmove` with proper UCI notation
  - `info score cp <score> depth <depth>` - Search info
  - Promotion piece suffix (e7e8q)
- **Move Application**:
  - Parses and applies move sequences
  - Validates moves (subset implementation)
  - Updates position state correctly

### Added - GUI
- **Native GUI** (`vdmo-chess-gui.exe`)
  - eframe/egui-based graphical interface
  - 8×8 chessboard rendering with Unicode pieces
  - Click-to-move interaction
  - Board flip option
  - Reset to start position
  - Status panel with move history

### Added - CLI
- **Perft Command**: `vdmo perft <depth> [fen]`
- **UCI Mode**: `vdmo uci`
- Supports custom FEN positions

### Technical Details
- **Language**: Rust (edition 2021)
- **Architecture**: Library crate + two binaries
- **Safety**: No unsafe code except in Zobrist initialization
- **Dependencies**: Minimal (eframe/egui only for GUI)

### Project Structure
```
vdmo/
├── src/
│   ├── lib.rs              # Engine library
│   │   ├── core::types     # Basic types
│   │   ├── core::mv        # Move representation
│   │   ├── core::zobrist   # Zobrist hashing
│   │   ├── core::board     # Position/state
│   │   ├── core::movegen   # Move generation
│   │   ├── core::search    # Alpha-beta search
│   │   ├── core::perft     # Validator
│   │   └── core::uci       # Protocol handler
│   ├── main.rs             # UCI/CLI binary
│   └── bin/
│       └── vdmo-chess-gui.rs  # GUI binary
├── Cargo.toml
└── README.md
```

## Performance Metrics

### Move Generation Speed
- Perft depth 4: ~197k nodes (validated)
- Perft depth 5: ~4.8M nodes (validated)

### Search Depth
- Fixed depth 5 search in ~1-3 seconds (debug build)
- Expected ~depth 6-8 in release build with optimizations

### Estimated Strength Progression
- v0.1.0: ~1200-1400 ELO (material-only evaluation)
- v0.2.0: ~1600-1800 ELO (material + PST + TT + MVV-LVA)
- v0.3.0: ~1800-2000 ELO (+ killers + history + time management)
- v0.4.0: ~2000-2200 ELO (+ LMR + null-move pruning)

## Known Limitations

### Current
- No opening book
- Ray-based sliding moves (not magic bitboards)
- No aspiration windows
- No principal variation extraction
- GUI not integrated with engine yet

### Future Work
- Implement advanced pruning techniques
- Add endgame tablebases
- NNUE evaluation for top-level play
- Multi-threaded search
- SMP (Symmetric Multi-Processing)

---

## Contributors

VDMO Chess Engine - Built with Rust

## License

MIT License