# safe-vanity

Mine **Safe (Gnosis Safe) vanity addresses** by varying `saltNonce` until the CREATE2-derived proxy address matches a pattern (prefix/suffix/contains). No elliptic curve math — only Keccak-256 hashing, making it fast on CPU and GPU-friendly.

## Quick Start

```bash
# 1. Build the miner
cargo build --release

# 2. Fetch your Safe config (factory, init_code_hash, initializer_hash)
cd verify-with-safe-sdk && npm install && cd ..
node verify-with-safe-sdk/fetch-config.js \
  --chain-id 11155111 \
  --owners "0xYourAddress" \
  --threshold 1 \
  --rpc-url https://ethereum-sepolia-rpc.publicnode.com

# 3. Mine a vanity address (paste the output from step 2)
./target/release/safe_vanity -p dead \
  --factory 0x4e1DCf7AD4e460CfD30791CCC4F9c8a4f820ec67 \
  --init-code-hash 0x76733d... \
  --initializer-hash 0xabcdef...

# 4. Verify the result
node verify-with-safe-sdk/verify.js \
  --factory 0x4e1D... \
  --init-code-hash 0x7673... \
  --initializer-hash 0xabcd... \
  --salt-nonce <HEX_FROM_MINER>

# 5. Deploy the Safe (optional)
node verify-with-safe-sdk/deploy.js \
  --chain-id 11155111 \
  --owners "0xYourAddress" \
  --threshold 1 \
  --salt-nonce <DECIMAL_FROM_MINER> \
  --rpc-url https://... \
  --private-key <YOUR_KEY> \
  --deploy
```

## How It Works

From [SafeProxyFactory.createProxyWithNonce](https://github.com/safe-global/safe-smart-account/blob/main/contracts/proxies/SafeProxyFactory.sol):

```
salt         = keccak256(initializerHash || saltNonce)     // 64 bytes -> 32 bytes
safe_address = keccak256(0xff || factory || salt || initCodeHash)[12:32]  // 85 bytes -> 20 bytes
```

You provide `factory`, `init_code_hash`, and `initializer_hash` (from the Safe SDK or `fetch-config.js`). The miner tries sequential `saltNonce` values across multiple CPU threads until the computed address matches your pattern.

**Output includes both hex and decimal salt nonce** — the decimal value can be pasted directly into the Safe SDK's `safeDeploymentConfig.saltNonce`.

## Prerequisites

- **Rust** 1.70+ (for the miner)
- **Node.js** 18+ (for config fetching, verification, and deployment)

## Build

```bash
cargo build --release
```

## Usage

### Pattern Types

```bash
# Prefix (default)
./target/release/safe_vanity -p dead --factory ... --init-code-hash ... --initializer-hash ...

# Suffix
./target/release/safe_vanity -p beef -t suffix --factory ... --init-code-hash ... --initializer-hash ...

# Contains
./target/release/safe_vanity -p cafe -t contains --factory ... --init-code-hash ... --initializer-hash ...

# Prefix + Suffix
./target/release/safe_vanity -p c0ffee -s 93 --factory ... --init-code-hash ... --initializer-hash ...
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-p, --pattern` | Hex pattern to search for | (required) |
| `-s, --suffix` | Suffix pattern (turns pattern into prefix) | - |
| `-t, --pattern-type` | `prefix`, `suffix`, `contains` | `prefix` |
| `--factory` | SafeProxyFactory address (20 bytes hex) | (required) |
| `--init-code-hash` | keccak256(creationCode \|\| singleton) (32 bytes hex) | (required) |
| `--initializer-hash` | keccak256(setup calldata) (32 bytes hex) | (required) |
| `-w, --workers` | Number of CPU threads | CPU count |
| `-n, --count` | Stop after N matches | 1 |
| `-c, --case-sensitive` | Case-sensitive matching | false |
| `-r, --report-interval` | Progress report interval (seconds) | 5 |

### Output Example

```
=== Match #1 ===
Address:      0xDEADfe2260d1D438D321BCc2BAec9609c8C49999
Salt (hex):   0xa60947a0b82cb1deb30424270413c4f022fa820eaefee651cf9c58ac5f37135f
Salt (dec):   75132...  (use this in Safe SDK)
Worker:       3
```

## Full Workflow

### Step 1: Fetch Config

The `fetch-config.js` script resolves all Safe deployment addresses automatically:

```bash
cd verify-with-safe-sdk && npm install
node fetch-config.js \
  --chain-id 11155111 \
  --owners "0xOwner1,0xOwner2" \
  --threshold 2 \
  --rpc-url https://ethereum-sepolia-rpc.publicnode.com
```

It auto-detects the CompatibilityFallbackHandler from `@safe-global/safe-deployments`. Override with `--fallback-handler <addr>` if needed.

### Step 2: Mine

Paste the `--factory`, `--init-code-hash`, and `--initializer-hash` from step 1:

```bash
./target/release/safe_vanity -p dead \
  --factory <from step 1> \
  --init-code-hash <from step 1> \
  --initializer-hash <from step 1>
```

### Step 3: Verify

Cross-check the miner's result with the JS CREATE2 formula:

```bash
node verify-with-safe-sdk/verify.js \
  --factory <same> --init-code-hash <same> --initializer-hash <same> \
  --salt-nonce <hex from miner>
```

Or verify against the Safe SDK:

```bash
node verify-with-safe-sdk/verify.js --sdk \
  --rpc-url https://... --owner 0x... --salt-nonce <decimal>
```

### Step 4: Deploy

```bash
node verify-with-safe-sdk/deploy.js \
  --chain-id 11155111 \
  --owners "0xOwner1,0xOwner2" \
  --threshold 2 \
  --salt-nonce <decimal from miner> \
  --rpc-url https://... \
  --private-key <deployer key> \
  --deploy
```

Omit `--deploy` for a dry run that only predicts the address.

## Tests

```bash
cargo test
```

## Project Layout

```
safe-vanity/
├── src/                          # Rust miner
│   ├── main.rs                   # CLI entry point, progress loop
│   ├── config.rs                 # Clap CLI args, validation
│   ├── crypto/create2.rs         # CREATE2 salt + address computation
│   ├── matcher/pattern.rs        # Zero-alloc nibble-level pattern matching
│   └── worker/                   # CPU workers, pool, stats
└── verify-with-safe-sdk/         # JS tools
    ├── fetch-config.js           # Fetch factory/init_code_hash/initializer_hash
    ├── verify.js                 # Verify results (formula + SDK)
    └── deploy.js                 # Deploy the Safe on-chain
```

## Performance

CREATE2 mining involves only Keccak-256 (no elliptic curve math), making it significantly faster than EOA vanity generation. Typical throughput on modern CPUs: **3-5M salts/sec per core**.

## References

- [Safe deployment — predict the Safe address](https://docs.safe.global/sdk/protocol-kit/guides/safe-deployment#predict-the-safe-address)
- [Protocol Kit init — predictedSafe](https://docs.safe.global/reference-sdk-protocol-kit/initialization/init#predictedsafe-optional)
- [SafeProxyFactory.sol](https://github.com/safe-global/safe-smart-account/blob/main/contracts/proxies/SafeProxyFactory.sol)
- [EIP-1014 CREATE2](https://eips.ethereum.org/EIPS/eip-1014)
- [safe-deployments](https://github.com/safe-global/safe-deployments)
