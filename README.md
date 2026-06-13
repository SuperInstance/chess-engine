# Chess Engine

A from-scratch **chess engine**: move generation, legal move validation, board representation, and search. The engine represents positions as a bitboard-based board state and generates moves using ray-based sliding piece attacks.

## Why It Matters

Chess engines are the canonical testbed for algorithm design. Every major technique in search (alpha-beta, iterative deepening, transposition tables), evaluation (material counting, positional scoring, neural networks), and optimization (bit manipulation, cache-friendly data structures) was refined in chess engines before being applied elsewhere. Stockfish, the strongest traditional engine, has been optimized by thousands of contributors over 20+ years and runs at 70+ million nodes/second.

Building a chess engine teaches: bit manipulation (bitboards are 64-bit integers, one bit per square), combinatorial search with pruning, adversarial game trees, and the tension between evaluation accuracy and speed. The same algorithms power game AI, planning systems, and decision-making under uncertainty.

## How It Works

**Bitboards**: The board is represented as twelve 64-bit integers (one per piece type/color), where each bit corresponds to a square (a1=bit0, h8=bit63). Bitwise operations generate attacks:
- Knights: precomputed lookup table indexed by square → mask of knight target squares. O(1).
- King: same — precomputed per-square mask.
- Sliding pieces (bishop, rook, queen): ray-casting from the square outward in each direction until hitting a blocker. This implementation uses the classical approach (loop-based ray extension). Advanced engines use magic bitboards — hash the occupancy bits and look up attack masks in precomputed tables for O(1) sliding move generation.
- Pawns: shift operations (white pawns shift left by 8, etc.).

**Move generation** produces pseudo-legal moves (king may be in check after), then filters for legality by verifying the king isn't attacked after each move.

**Evaluation**: Material balance (pawn=100, knight=320, bishop=330, rook=500, queen=900) plus piece-square tables that encode positional preferences (e.g., knights are worth more in the center, less on the rim).

**Search**: Alpha-beta pruning with iterative deepening. The engine searches to depth D, then D+1, etc., using the previous depth's best move as the first move searched (improves pruning efficiency — best moves cause more cutoffs).

## Quick Start

```rust
use chess_engine::Board;

let board = Board::starting_position();
let moves = board.legal_moves();
println!("{} legal moves", moves.len());

// Make a move
let board = board.make_move(&moves[0]);

// Search to depth 4
let (best_move, score) = board.search(4);
println!("Best move: {:?}, score: {}", best_move, score);
```

## API

- `Board::starting_position()` — Standard chess starting position
- `board.legal_moves()` — All legal moves for the side to move
- `board.make_move(mv)` — Returns new board state after move
- `board.search(depth)` — Alpha-beta search, returns best move and evaluation
- `Move` — Source square, destination square, promotion piece (if any)

## Architecture Notes

Part of the [SuperInstance](https://github.com/SuperInstance) ecosystem. Chess search and the fleet's build harness share a deep structure: both explore exponentially growing trees, both use domain knowledge to prune, and both must balance search breadth (explore alternatives) against depth (commit to promising paths). This is the conservation law γ + η = C applied to computation — spend γ (search nodes) to produce η (better decisions).

See [ARCHITECTURE.md](https://github.com/SuperInstance/SuperInstance/blob/main/ARCHITECTURE.md).

## References

- Shannon, C. E. (1950). "Programming a Computer for Playing Chess." — the original paper that started computer chess
- Russell, S. & Norvig, P. (2020). *AI: A Modern Approach*, Ch. 5. "Adversarial Search." — alpha-beta theory
- Stockfish docs: [chessprogramming.org](https://www.chessprogramming.org/) — community wiki with every technique catalogued

## License

MIT
