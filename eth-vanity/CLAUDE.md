# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Build (release mode for performance)
cargo build --release

# Run tests
cargo test

# Run a single test
            cargo test test_deterministic_address

# Run with pattern
./target/release/eth_vanity -p dead           # prefix match
./target/release/eth_vanity -p beef -t suffix # suffix match
./target/release/eth_vanity -p cafe -t contains -n 5
```

## Architecture

This is a multi-threaded Ethereum vanity address generator. The data flow is:

```
main.rs (CLI) → WorkerPool → [CpuWorker threads] → Keypair::generate() → Pattern::matches()
                    ↑                                      ↓
              crossbeam channel ←────────── VanityResult (on match)
```

**Key architectural decisions:**

1. **Worker coordination**: `WorkerPool` spawns N threads, each running a `CpuWorker`. Workers share an `Arc<AtomicBool>` stop flag and `Arc<WorkerStats>` for lock-free statistics aggregation.

2. **Result delivery**: Workers send matches through a bounded crossbeam channel (capacity 100). The main thread polls with timeout to interleave progress reporting.

3. **Graceful shutdown**: `WorkerPool::Drop` ensures threads are joined. The stop flag is exposed via `stop_flag_clone()` for Ctrl+C handling.

4. **Address derivation** (in `crypto/keypair.rs`): secp256k1 keypair → uncompressed public key (drop 0x04 prefix) → Keccak-256 → last 20 bytes.

## Extending with GPU Workers

The `worker/` module is designed for extension. To add GPU support:
1. Create `worker/gpu.rs` implementing the same pattern as `cpu.rs`
2. Add a `GpuWorker` that sends results to the same channel type
3. `WorkerPool` can manage mixed CPU/GPU workers
