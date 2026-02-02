# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Build CPU-only (default, no OpenCL needed)
cargo build --release

# Build with GPU support (requires OpenCL runtime)
cargo build --release --features gpu

# Run tests
cargo test                  # CPU-only tests (11 tests)
cargo test --features gpu   # All tests including GPU unit tests (16 tests)

# Run a single test
cargo test test_deterministic_address

# Run with pattern
./target/release/eth_vanity -p dead                        # prefix match
./target/release/eth_vanity -p beef -t suffix              # suffix match
./target/release/eth_vanity -p cafe -t contains -n 5       # contains, find 5
./target/release/eth_vanity -p c0ffee -s 93                # prefix + suffix
./target/release/eth_vanity -p dead --gpu                  # GPU-accelerated
./target/release/eth_vanity -p dead --gpu --gpu-work-size 2097152  # larger batch
```

## Project Structure

```
eth-vanity/
├── Cargo.toml                # Dependencies, [features] section with `gpu` flag
├── kernels/
│   └── vanity.cl             # OpenCL kernel (secp256k1 math, keccak256, pattern matching)
└── src/
    ├── main.rs               # CLI entry point, progress loop, result printing
    ├── lib.rs                # Public API re-exports
    ├── config.rs             # Clap CLI args, validation, GPU flags (cfg-gated)
    ├── crypto/
    │   ├── mod.rs
    │   ├── keypair.rs        # secp256k1 key generation, address derivation
    │   └── address.rs        # 20-byte address type, hex/checksum formatting
    ├── matcher/
    │   ├── mod.rs
    │   └── pattern.rs        # Pattern/PatternType/MatchResult, prefix/suffix/contains matching
    └── worker/
        ├── mod.rs            # Module exports, conditional `gpu` module
        ├── cpu.rs            # CpuWorker (random keypair loop), WorkerStats (atomic counters)
        ├── gpu.rs            # GpuWorker (OpenCL incremental key approach) [feature = "gpu"]
        └── pool.rs           # WorkerPool (thread management, channel, stats aggregation)
```

## Architecture

### Data Flow

```
main.rs (CLI parsing + progress loop)
  │
  ▼
WorkerPool
  ├── CpuWorker thread 0 ──┐
  ├── CpuWorker thread 1 ──┤
  ├── ...                   ├──→ crossbeam channel (bounded, cap 100) ──→ main loop
  ├── CpuWorker thread N ──┤                                              │
  └── GpuWorker thread ────┘  (optional, feature = "gpu")                 ▼
                                                                    print result
       Shared state:                                                or progress
       - Arc<AtomicBool> stop_flag
       - Arc<WorkerStats> {keys_generated, matches_found}
```

### CPU Worker (`worker/cpu.rs`)

Each `CpuWorker` runs a tight loop:
1. `Keypair::generate()` — random secp256k1 key via CSPRNG
2. `Pattern::matches(address)` — test against target pattern
3. On match: send `VanityResult` through channel
4. Every 1000 keys: update `WorkerStats` atomic counter
5. Check `stop_flag` between batches

### GPU Worker (`worker/gpu.rs`, feature-gated)

Uses the **incremental key approach** (same as profanity2):

1. CPU generates random base private key `k` via CSPRNG
2. CPU computes base public key `Q = k * G` using secp256k1 crate
3. CPU sends `Q` (64 bytes) + precomputed `2^i * G` table (32 entries) to GPU
4. Each GPU work item `i` (out of ~1M) computes:
   - `offset_point = i * G` (via precomputed table, bit-decomposition of `i`)
   - `Q' = Q + offset_point` (EC point addition in affine coordinates)
   - `addr = keccak256(serialize(Q'))[12..32]`
   - Pattern match on `addr` nibbles
5. Matching offsets written to result buffer with atomic counter
6. CPU reads results, reconstructs private key: `(k + offset) mod n`
7. CPU verifies result via `Keypair::from_secret_key()`, sends through same channel

Key implementation details:
- Kernel source loaded via `include_str!("../../kernels/vanity.cl")`
- `GpuPatternConfig` is a `#[repr(C, packed)]` struct matching the kernel's layout
- G table: 32 entries of `2^k * G` for k=0..31, each 64 bytes (x||y, big-endian)
- `add_scalar_mod_n()`: 256-bit addition modulo secp256k1 curve order
- Graceful fallback: if GPU init fails, prints warning and continues CPU-only

### OpenCL Kernel (`kernels/vanity.cl`)

The kernel implements from scratch (no external dependencies):
- **256-bit arithmetic**: add, sub, gte, eq (8 x 32-bit limbs, little-endian)
- **Fp arithmetic**: add, sub, mul, sqr, inv modulo secp256k1 prime `p`
  - Multiplication uses schoolbook 8x8 → 16 limbs, then fast reduction via `p = 2^256 - 0x1000003D1`
  - Inversion via Fermat's little theorem `a^(p-2)` with optimized addition chain
- **EC operations**: affine point addition and doubling
- **Scalar mul**: decompose 32-bit offset into bits, add precomputed `2^k * G` entries
- **Keccak-256**: full keccak-f[1600] permutation, hardcoded for 64-byte input
  - Uses padding byte `0x01` (Keccak), NOT `0x06` (SHA3)
- **Pattern matching**: prefix, suffix, contains, prefix+suffix on address nibbles

### Worker Pool (`worker/pool.rs`)

- `new(num_workers, pattern)` — CPU-only mode
- `new_with_gpu(num_cpu, pattern, enable_gpu, device, work_size)` — mixed CPU+GPU mode
- All workers share the same `Sender<VanityResult>` channel and `WorkerStats`
- `Drop` impl ensures all threads are joined on cleanup
- `stop_flag_clone()` exposed for Ctrl+C handler in main

### Pattern Matching (`matcher/pattern.rs`)

- `PatternType`: Prefix, Suffix, Contains, PrefixAndSuffix
- Operates on the 40-nibble hex representation of the 20-byte address
- Case insensitive by default (all hex lowercased)
- `estimated_difficulty()`: `16^n` where n = total pattern nibble length

### Config (`config.rs`)

- Clap derive-based CLI parsing
- GPU flags (`--gpu`, `--gpu-device`, `--gpu-work-size`) gated behind `#[cfg(feature = "gpu")]`
- Accessor methods (`gpu_enabled()`, etc.) return safe defaults when compiled without GPU

## Adding CREATE2 Salt Mining

CREATE2 address computation: `keccak256(0xff ++ deployer[20] ++ salt[32] ++ init_code_hash[32])[12:]`

This is a natural extension that reuses most of the existing infrastructure. Here's what to add:

### New CLI flags in `config.rs`
- `--create2` — enable CREATE2 mode
- `--deployer <hex>` — 20-byte deployer address
- `--init-code-hash <hex>` — 32-byte keccak256 of contract init code

### New kernel function in `kernels/vanity.cl`
Add a `create2_iterate_and_match` entry point (~30 lines). It reuses the existing keccak256 and pattern matching code. The only difference from the current kernel:
- No EC math at all — just construct an 85-byte preimage per work item
- The salt is `base_salt + global_id` (simple 256-bit increment)
- Hash the 85-byte preimage (needs a `keccak256_85bytes` variant since the current one is hardcoded for 64 bytes — the absorb phase handles 85 bytes within the 136-byte rate, so it's a small change to the padding offset)
- Pattern match on `hash[12..32]` (same as current)

### New worker in `worker/`
Either a `Create2GpuWorker` or a mode enum on the existing `GpuWorker`. Key differences:
- No G table or EC operations on the CPU side
- Sends `deployer` (20 bytes) + `init_code_hash` (32 bytes) to GPU instead of `base_pubkey` + `g_table`
- Result is the matching salt (32 bytes) instead of a private key
- No `Keypair::from_secret_key` verification — just recompute keccak256 on CPU to verify

### `VanityResult` changes
Add a variant or field for the salt. The `private_key` field could be repurposed, or add a `salt: Option<String>` field. The display in `main.rs` would show "Salt" instead of "Private Key" in CREATE2 mode.

### Performance note
CREATE2 mining is pure keccak256 (no EC math), so it will be significantly faster than EOA vanity generation on both CPU and GPU. The GPU kernel becomes memory-bandwidth bound rather than compute-bound.
