# eth-vanity

High-performance Ethereum vanity address generator with optional GPU acceleration.

Generates Ethereum keypairs and checks if the resulting address matches a target pattern (prefix, suffix, contains, or both). Uses multi-threaded CPU workers by default, with an optional OpenCL GPU backend that uses the incremental key approach for parallel address generation.

## Prerequisites

- **Rust** (1.70+): https://rustup.rs
- **OpenCL runtime** (only if using `--features gpu`):
  - NVIDIA: install CUDA toolkit or `nvidia-opencl-icd`
  - AMD: install ROCm or AMDGPU-PRO drivers
  - Intel: install `intel-opencl-icd` or oneAPI
  - On Debian/Ubuntu: `sudo apt install ocl-icd-opencl-dev` for headers

## Building

```bash
# CPU-only (default — no OpenCL dependency)
cargo build --release

# With GPU acceleration
cargo build --release --features gpu
```

## Usage

```bash
# Find address starting with "dead"
./target/release/eth_vanity -p dead

# Find address ending with "beef"
./target/release/eth_vanity -p beef -t suffix

# Find address containing "cafe", find 5 matches
./target/release/eth_vanity -p cafe -t contains -n 5

# Find address starting with "c0ffee" and ending with "93"
./target/release/eth_vanity -p c0ffee -s 93

# Use GPU acceleration
./target/release/eth_vanity -p c0ffee --gpu

# GPU with custom device and batch size
./target/release/eth_vanity -p c0ffee --gpu --gpu-device 0 --gpu-work-size 2097152

# Run forever (useful for collecting multiple rare addresses)
./target/release/eth_vanity -p deadbeef -n 0
```

### All Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--pattern` | `-p` | required | Hex pattern to search for (0-9, a-f) |
| `--suffix` | `-s` | — | Suffix pattern (enables prefix+suffix mode) |
| `--pattern-type` | `-t` | `prefix` | Match type: `prefix`, `suffix`, `contains` |
| `--workers` | `-w` | CPU count | Number of CPU worker threads |
| `--case-sensitive` | `-c` | `false` | Case sensitive matching |
| `--count` | `-n` | `1` | Stop after N matches (0 = run forever) |
| `--report-interval` | `-r` | `5` | Progress report interval in seconds |
| `--gpu` | — | `false` | Enable GPU acceleration (requires `gpu` feature) |
| `--gpu-device` | — | `0` | OpenCL GPU device index |
| `--gpu-work-size` | — | `1048576` | Keys per GPU batch (2^20) |

## Example Output

```
Ethereum Vanity Address Generator
==================================
Pattern:    dead (prefix)
Difficulty: Medium (minutes)
Workers:    8
Target:     1 address(es)

Searching... (Press Ctrl+C to stop)

[   5s] Generated 2.45M keys (489.12K/s)
=== Match #1 ===
Address:     0xDeaD1a2B3c4D5e6F7a8B9c0D1e2F3a4B5c6D7e8F
Private Key: 4a2f...c8b1
Worker:      3

Target reached! Found 1 address(es).

--- Final Statistics ---
Total keys generated: 3.21M
Total matches found:  1
Time elapsed:         6.57s
Average speed:        489.12K/s
```

## Difficulty Estimates

Each hex character multiplies the search space by 16x:

| Pattern Length | Expected Attempts | Rough Time (500K keys/s) |
|---------------|-------------------|--------------------------|
| 1 char | 16 | instant |
| 2 chars | 256 | instant |
| 3 chars | 4,096 | instant |
| 4 chars | 65,536 | < 1 second |
| 5 chars | 1,048,576 | ~2 seconds |
| 6 chars | 16,777,216 | ~34 seconds |
| 7 chars | 268,435,456 | ~9 minutes |
| 8 chars | 4,294,967,296 | ~2.4 hours |

GPU acceleration can increase throughput by 10-100x depending on hardware.

## Running Tests

```bash
# CPU-only tests
cargo test

# All tests (including GPU unit tests)
cargo test --features gpu
```

## Architecture

See [CLAUDE.md](CLAUDE.md) for detailed architecture documentation, including the GPU incremental key approach, OpenCL kernel internals, and a guide for implementing CREATE2 salt mining.

## License

MIT
