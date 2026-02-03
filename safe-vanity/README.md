# safe-vanity

Mine **Safe (Gnosis Safe) vanity addresses** by varying `saltNonce` until the CREATE2-derived proxy address matches a pattern (prefix/suffix/contains). No elliptic curve math — only Keccak-256 hashing, making it fast on CPU and GPU-friendly.

## Layout

```
safe-vanity/
├── miner/                    # Rust miner (safe_vanity binary)
│   ├── Cargo.toml
│   └── src/
└── executor/                 # JS: fetch config, run miner, verify, deploy
    ├── run.js                # Single flow: mine + optional deploy
    ├── deploy.js             # Deploy with existing nonce
    ├── fetch-config.js
    ├── verify.js
    └── lib/
```

## Quick Start

```bash
# 1. Build the miner
cargo build --release
# Binary: target/release/safe_vanity (workspace) or miner/target/release/safe_vanity

# 2. Run single flow (fetch → mine → verify; optional deploy)
cd executor && npm install && cd ..
node executor/run.js --owners "0xYourAddress" --threshold 1 --pattern dead

# 3. Or deploy after mining (confirmation prompt)
node executor/run.js --owners "0xYourAddress" --threshold 1 --pattern dead --deploy
# Private key: --private-key <hex> or SAFE_DEPLOYER_PRIVATE_KEY in .env
```

## How It Works

From [SafeProxyFactory.createProxyWithNonce](https://github.com/safe-global/safe-smart-account/blob/main/contracts/proxies/SafeProxyFactory.sol):

```
salt         = keccak256(initializerHash || saltNonce)     // 64 bytes -> 32 bytes
safe_address = keccak256(0xff || factory || salt || initCodeHash)[12:32]  // 85 bytes -> 20 bytes
```

The **miner** (Rust) tries sequential `saltNonce` values until the computed address matches your pattern. The **executor** (JS) fetches factory/init_code_hash/initializer_hash, runs the miner, verifies the result, and can deploy.

## Miner (Rust)

```bash
cargo build -p safe_vanity --release
./target/release/safe_vanity -p dead --factory ... --init-code-hash ... --initializer-hash ...
# Or from miner dir: cargo run -p safe_vanity -- -p dead --factory ...
```

See `miner/` for pattern options (`-p`, `-s`, `-t`, `-c`), workers, count, etc.

## Executor (JS)

From repo root or `executor/`:

```bash
cd executor && npm install
node run.js --owners "0x...,0x..." --threshold 1 --pattern dead
node run.js ... --pattern dead --deploy   # deploy after mining (confirmation)
node deploy.js --owners ... --threshold 1 --salt-nonce 12345 --deploy   # deploy with existing nonce
node fetch-config.js --owners ... --threshold 1
node verify.js --factory ... --init-code-hash ... --initializer-hash ... --salt-nonce ...
```

Config: `executor/safe-vanity.config.json` (chainId, rpcUrl, useL2, safeVersion, etc.). See `executor/README.md` for full docs.

## Tests

```bash
cargo test -p safe_vanity
```

## References

- [Safe deployment — predict the Safe address](https://docs.safe.global/sdk/protocol-kit/guides/safe-deployment#predict-the-safe-address)
- [SafeProxyFactory.sol](https://github.com/safe-global/safe-smart-account/blob/main/contracts/proxies/SafeProxyFactory.sol)
- [EIP-1014 CREATE2](https://eips.ethereum.org/EIPS/eip-1014)
- [safe-deployments](https://github.com/safe-global/safe-deployments)
