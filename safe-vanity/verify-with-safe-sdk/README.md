# Safe Vanity — JS Tools

Single-flow: config → fetch Safe params → mine → verify. Only **owners**, **threshold**, and **pattern** are required; everything else comes from `safe-vanity.config.json`.

## Install

```bash
npm install
```

Build the Rust miner (from `safe-vanity` repo root):

```bash
cargo build --release
# optional: cargo install --path .  → puts safe_vanity on PATH
```

## Config

Defaults live in `safe-vanity.config.json`:

| Field | Default | Description |
|-------|---------|-------------|
| `chainId` | `"11155111"` | Chain ID (Sepolia) |
| `rpcUrl` | Sepolia RPC | RPC URL for fetch & deploy |
| `useL2` | `false` | Use SafeL2 singleton |
| `safeVersion` | `"1.4.1"` | Safe version filter |
| `fallbackHandler` | `null` | Override or auto-detect |
| `minerPath` | `"safe_vanity"` | Miner binary (PATH or path) |
| `workers` | `null` | Miner worker threads |
| `count` | `1` | Stop after N matches |
| `reportInterval` | `5` | Progress interval (seconds) |

Override via CLI flags or a different config file (`--config <path>`).

## Single flow (mine + verify)

Only **owners**, **threshold**, and **pattern** are required. Chain, RPC, L2, Safe version, etc. come from config.

```bash
node run.js --owners "0x...,0x..." --threshold 1 --pattern dead
```

This will:

1. Load `safe-vanity.config.json` (or `--config <path>`)
2. Fetch factory, init_code_hash, initializer_hash for the chain/Safe version
3. Run `safe_vanity` with that config and your pattern
4. When the miner finds a match, verify the address with the CREATE2 formula and print the deploy command

Override config from CLI:

```bash
node run.js --owners "0x..." --threshold 1 -p cafe --chain-id 1 --rpc-url https://... --l2 --safe-version 1.4.1
```

## Fetch config only

Print factory, init_code_hash, initializer_hash (and example commands). Only **owners** and **threshold** required; rest from config.

```bash
node fetch-config.js --owners "0x...,0x..." --threshold 1
```

## Verify (formula)

Cross-check miner output with the CREATE2 formula. Salt-nonce can be **decimal** (from miner “Salt (dec):”) or 64-char hex.

```bash
node verify.js --factory <40-hex> --init-code-hash <64-hex> --initializer-hash <64-hex> --salt-nonce <decimal-or-64-hex>
```

## Deploy

Deploy a mined Safe. Only **owners**, **threshold**, and **salt-nonce** required; chain/RPC from config.

```bash
# Dry run (predict address only)
node deploy.js --owners "0x...,0x..." --threshold 1 --salt-nonce 12345

# Deploy on-chain
node deploy.js --owners "0x...,0x..." --threshold 1 --salt-nonce 12345 --deploy --private-key <hex>
```

## References

- [Safe deployment — predict the Safe address](https://docs.safe.global/sdk/protocol-kit/guides/safe-deployment#predict-the-safe-address)
- [safe-deployments](https://github.com/safe-global/safe-deployments)
- [SafeProxyFactory.sol](https://github.com/safe-global/safe-smart-account/blob/main/contracts/proxies/SafeProxyFactory.sol)
