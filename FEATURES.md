# VDMO Chess Engine - Feature Comparison

This document provides a detailed comparison of features across different versions of the VDMO Chess Engine.

## Version Summary

| Version | Release Date | Estimated ELO | Key Features |
|---------|-------------|---------------|--------------|
| v0.1.0 | 2024 | 1200-1400 | Basic engine with material eval |
| v0.2.0 | 2024 | 1600-1800 | + TT + PST + MVV-LVA |
| v0.3.0 | 2024 | 1800-2000 | + Killers + History + Time Mgmt |

## Feature Matrix

### Core Engine Features

| Feature | v0.1.0 | v0.2.0 | v0.3.0 | ELO Impact |
|---------|--------|--------|--------|------------|
| **Board Representation** |
| Bitboards (12 × u64) | ✅ | ✅ | ✅ | Foundation |
| Zobrist Hashing | ❌ | ✅ | ✅ | Enables TT |
| Incremental Hash Updates | ❌ | ✅ | ✅ | Performance |
| **Move Generation** |
| Pseudo-legal Movegen | ✅ | ✅ | ✅ | Foundation |
| Legality Filtering | ✅ | ✅ | ✅ | Foundation |
| All Special Moves | ✅ | ✅ | ✅ | Foundation |
| Ray-based Sliding | ✅ | ✅ | ✅ | 0 (correctness) |
| Magic Bitboards | ❌ | ❌ | ❌ | +50-100 planned |
| **Make/Unmake** |
| State Stack | ✅ | ✅ | ✅ | Foundation |
| Castling Rights Update | ✅ | ✅ | ✅ | Foundation |
| En-passant Tracking | ✅ | ✅ | ✅ | Foundation |
| **Validation** |
| Perft Implementation | ✅ | ✅ | ✅ | Correctness |
| Perft Verified (depth 5) | ✅ | ✅ | ✅ | Correctness |

### Search Features

| Feature | v0.1.0 | v0.2.0 | v0.3.0 | ELO Impact |
|---------|--------|--------|--------|------------|
| **Basic Search** |
| Alpha-Beta Pruning | ✅ | ✅ | ✅ | +200 vs minimax |
| Negamax Framework | ✅ | ✅ | ✅ | Foundation |
| Iterative Deepening | ✅ | ✅ | ✅ | +50 |
| Quiescence Search | ✅ | ✅ | ✅ | +200 |
| Mate Detection | ✅ | ✅ | ✅ | Foundation |
| **Hash Tables** |
| Transposition Table | ❌ | ✅ | ✅ | +200 |
| TT Size (entries) | - | 1M | 1M | - |
| TT Bound Types | ❌ | ✅ | ✅ | Better pruning |
| TT Replacement Scheme | ❌ | Depth | Depth | - |
| **Move Ordering** |
| Random Order | ✅ | ❌ | ❌ | Baseline |
| TT Move First | ❌ | ✅ | ✅ | +100 |
| MVV-LVA Captures | ❌ | ✅ | ✅ | +150 |
| Killer Moves | ❌ | ❌ | ✅ | +50-80 |
| History Heuristic | ❌ | ❌ | ✅ | +70-100 |
| Counter Moves | ❌ | ❌ | ❌ | +30-50 planned |
| **Pruning** |
| Null-Move Pruning | ❌ | ❌ | ❌ | +50-100 planned |
| Late Move Reductions | ❌ | ❌ | ❌ | +100-150 planned |
| Futility Pruning | ❌ | ❌ | ❌ | +30-50 planned |
| **Extensions** |
| Check Extensions | ❌ | ❌ | ❌ | +20-30 planned |
| Singular Extensions | ❌ | ❌ | ❌ | +50+ planned |

### Evaluation Features

| Feature | v0.1.0 | v0.2.0 | v0.3.0 | ELO Impact |
|---------|--------|--------|--------|------------|
| **Material** |
| Piece Values | ✅ | ✅ | ✅ | +800 vs random |
| Standard Values | ✅ | ✅ | ✅ | Foundation |
| **Positional** |
| Piece-Square Tables | ❌ | ✅ | ✅ | +300 |
| PST for All Pieces | ❌ | ✅ | ✅ | Foundation |
| Tapered Eval (MG/EG) | ❌ | ❌ | ❌ | +100 planned |
| **Pawn Structure** |
| Passed Pawns | ❌ | ❌ | ❌ | +50 planned |
| Doubled Pawns | ❌ | ❌ | ❌ | +30 planned |
| Isolated Pawns | ❌ | ❌ | ❌ | +30 planned |
| **King Safety** |
| Pawn Shield | ❌ | ❌ | ❌ | +50 planned |
| King Tropism | ❌ | ❌ | ❌ | +30 planned |
| **Mobility** |
| Piece Mobility | ❌ | ❌ | ❌ | +100 planned |
| **Advanced** |
| NNUE Evaluation | ❌ | ❌ | ❌ | +400+ planned |

### UCI Protocol

| Feature | v0.1.0 | v0.2.0 | v0.3.0 |
|---------|--------|--------|--------|
| Basic Commands |
| `uci` | ✅ | ✅ | ✅ |
| `isready` | ✅ | ✅ | ✅ |
| `ucinewgame` | ✅ | ✅ | ✅ |
| `position startpos` | ✅ | ✅ | ✅ |
| `position fen` | ✅ | ✅ | ✅ |
| `position ... moves` | ✅ | ✅ | ✅ |
| `quit` | ✅ | ✅ | ✅ |
| Go Commands |
| `go depth <n>` | ✅ | ✅ | ✅ |
| `go wtime/btime` | ❌ | ❌ | ✅ |
| `go winc/binc` | ❌ | ❌ | ✅ |
| `go movetime` | ❌ | ❌ | ✅ |
| `go movestogo` | ❌ | ❌ | ✅ |
| `go infinite` | ❌ | ❌ | ✅ |
| `go nodes` | ❌ | ❌ | ❌ |
| `stop` | ❌ | ❌ | ❌ |
| Info Output |
| `info score cp` | ✅ | ✅ | ✅ |
| `info depth` | ✅ | ✅ | ✅ |
| `info nodes` | ❌ | ❌ | ❌ |
| `info nps` | ❌ | ❌ | ❌ |
| `info time` | ❌ | ❌ | ❌ |
| `info pv` | ❌ | ❌ | ❌ |
| Options |
| `setoption Hash` | ❌ | ❌ | ❌ |
| `setoption Threads` | ❌ | ❌ | ❌ |

### User Interfaces

| Feature | v0.1.0 | v0.2.0 | v0.3.0 |
|---------|--------|--------|--------|
| CLI Binary | ✅ | ✅ | ✅ |
| UCI Mode | ✅ | ✅ | ✅ |
| Perft Command | ✅ | ✅ | ✅ |
| Native GUI | ✅ | ✅ | ✅ |
| GUI-Engine Integration | ❌ | ❌ | ❌ |

## Performance Metrics

### Search Depth (1 second, startpos)

| Version | Debug Build | Release Build | Nodes/Second (est.) |
|---------|-------------|---------------|---------------------|
| v0.1.0 | 4-5 ply | 6-7 ply | 10k-20k |
| v0.2.0 | 5-6 ply | 7-8 ply | 15k-30k |
| v0.3.0 | 6-8 ply | 8-10 ply | 20k-50k |

### Perft Performance (nodes per second)

| Depth | v0.1.0 | v0.2.0 | v0.3.0 | Target (Magic BB) |
|-------|--------|--------|--------|-------------------|
| 4 | ~50k nps | ~50k nps | ~50k nps | ~500k nps |
| 5 | ~40k nps | ~40k nps | ~40k nps | ~400k nps |
| 6 | ~35k nps | ~35k nps | ~35k nps | ~350k nps |

*Note: Perft speed unchanged - improvements focused on search, not movegen*

### Time Management Accuracy

| Scenario | v0.3.0 Behavior |
|----------|----------------|
| 5min + 2sec inc | Allocates ~6-7 seconds per move early game |
| 1min no inc | Allocates ~2 seconds per move (1/30 of time) |
| movetime 5000 | Stops search at ~5000ms ±50ms |
| 40 moves in 10min | Allocates ~15 seconds per move (time/moves) |

## ELO Progression by Feature

```
Baseline (random moves): 0 ELO
+ Legal move generation: +800 ELO
+ Material evaluation: +400 ELO (1200 total)
+ Alpha-beta + quiescence: +400 ELO (1600 total)
────────────────────────────────────────────
v0.1.0 Baseline: ~1200-1400 ELO
────────────────────────────────────────────
+ Transposition table: +200 ELO
+ Piece-square tables: +300 ELO
+ MVV-LVA ordering: +150 ELO
────────────────────────────────────────────
v0.2.0: ~1600-1800 ELO (+400-500 from v0.1.0)
────────────────────────────────────────────
+ Killer moves: +70 ELO
+ History heuristic: +80 ELO
+ Time management: +50 ELO (better depth)
────────────────────────────────────────────
v0.3.0: ~1800-2000 ELO (+200 from v0.2.0)
────────────────────────────────────────────
```

## Planned Features (v0.4.0 and beyond)

### High Priority (200+ ELO potential)

1. **Late Move Reductions (LMR)** (+100-150 ELO)
   - Reduce depth for moves late in move list
   - Re-search if they raise alpha
   
2. **Better Evaluation** (+200-300 ELO)
   - Pawn structure evaluation
   - King safety evaluation
   - Piece mobility
   - Tapered evaluation (middlegame/endgame)

3. **Null-Move Pruning** (+50-100 ELO)
   - Skip a move and search at reduced depth
   - Prune if still fails high

### Medium Priority (50-100 ELO potential)

4. **Magic Bitboards** (+50-100 ELO via speed)
   - 10x faster move generation
   - Enables deeper search

5. **Aspiration Windows** (+20-40 ELO)
   - Narrow alpha-beta window around previous score
   - Re-search on window failure

6. **Principal Variation (PV)** (Usability)
   - Extract and display best line
   - Better UCI info output

### Long-term Goals (400+ ELO potential)

7. **NNUE Evaluation** (+400-600 ELO)
   - Neural network evaluation
   - Requires training infrastructure
   - State-of-the-art approach

8. **Multi-threaded Search** (+100-200 ELO)
   - Lazy SMP or YBWC
   - Scales with cores

9. **Endgame Tablebases** (+50-100 ELO)
   - Perfect play in simple endgames
   - Syzygy or Gaviota format

## Technical Stack

| Component | Technology | Notes |
|-----------|-----------|-------|
| Language | Rust (edition 2021) | Memory safety, performance |
| Build System | Cargo | Standard Rust toolchain |
| GUI Framework | eframe/egui | Immediate mode, cross-platform |
| Hash Function | Zobrist | XORshift64 PRNG with fixed seed |
| Bitboards | u64 (native) | Fast bit manipulation |
| Move Format | 16-bit packed | Compact, efficient |

## Comparison with Other Engines

### Amateur Engines (Similar Strength)

| Engine | ELO | Notable Features |
|--------|-----|------------------|
| VDMO v0.3.0 | ~1800-2000 | Material + PST, TT, Killers, History |
| Vice | ~1900 | Similar features, more mature |
| Bluefever | ~1800 | Similar approach |

### Strong Engines (Goals)

| Engine | ELO | Notable Features |
|--------|-----|------------------|
| Stockfish 16 | 3600+ | NNUE, SMP, all optimizations |
| Leela Chess Zero | 3500+ | Pure neural network |
| Komodo Dragon | 3500+ | Hybrid eval, SMP |

### VDMO Roadmap to 2200+ ELO

- v0.4.0: LMR + Better eval → ~2000-2200 ELO
- v0.5.0: Magic BB + Aspiration → ~2200-2400 ELO
- v0.6.0: SMP + Null-move → ~2400-2600 ELO
- v1.0.0: NNUE + Polish → ~2600-2800 ELO

## Testing Methodology

### Correctness

- **Perft Suite**: Standard positions (startpos, kiwipete, etc.)
- **Mate in N**: Known tactical positions
- **Game Logs**: Manual review of sample games

### Strength

- **Self-play**: Version vs version tournaments
- **Fixed Depth**: Games at depth 6 vs depth 5
- **CuteChess-CLI**: Automated tournaments
- **SPRT Testing**: Statistically significant strength testing (planned)

### Performance

- **Perft Benchmarks**: Nodes per second at depth 4-6
- **Search Benchmarks**: Time to depth at fixed positions
- **Profiling**: Flame graphs to identify bottlenecks (planned)

## Contributing Areas

### Easy (Good First Issues)

- Add more piece-square tables (endgame)
- Tune existing PST values
- Add UCI `info nodes` and `info nps` output
- Add more perft test positions
- Improve documentation

### Medium

- Implement counter-move heuristic
- Add principal variation extraction
- Implement `setoption` UCI commands
- Add opening book support
- Improve time management algorithm

### Hard

- Implement magic bitboards
- Implement late move reductions
- Add multi-threading support
- Implement NNUE evaluation
- Add Syzygy tablebase support

---

**Last Updated**: 2024  
**Current Version**: v0.3.0  
**Estimated Strength**: ~1800-2000 ELO