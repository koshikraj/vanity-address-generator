# Safe Vanity — JS Tools

Single-flow: config → fetch Safe params → mine → verify. Only **owners**, **threshold**, and **pattern** are required; everything else comes from `safe-vanity.config.json`.

## Install

```bash
npm install
```

Build the Rust miner (from `safe-vanity` repo root; miner lives in `miner/`):

```bash
cargo build -p safe_vanity --release
# Binary: target/release/safe_vanity. Or: cd miner && cargo build --release
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

## Single flow (mine + verify, optional deploy)

Only **owners**, **threshold**, and **pattern** are required. Add **--deploy** to deploy after mining; you get a confirmation prompt and need **--private-key** or `SAFE_DEPLOYER_PRIVATE_KEY`.

```bash
node run.js --owners "0x...,0x..." --threshold 1 --pattern dead
```

With deploy (confirmation prompt before sending tx):

```bash
node run.js --owners "0x...,0x..." --threshold 1 --pattern dead --deploy --private-key <hex>
# or: SAFE_DEPLOYER_PRIVATE_KEY=<hex> node run.js ... --deploy
```

Flow:

1. Load config, fetch factory/init_code_hash/initializer_hash
2. Run miner; on match, verify address via `predictSafeAddress` (reusable API)
3. If **--deploy**: show details and prompt "Deploy this Safe? (y/N)"; on yes, deploy
4. If not **--deploy**: print the `deploy.js` command so you can deploy later with the same nonce

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

## Deploy (standalone)

Use **deploy.js** when you already have a salt nonce (from miner output or API) and want to deploy without running the miner.

```bash
# Dry run (predict address only)
node deploy.js --owners "0x...,0x..." --threshold 1 --salt-nonce 12345

# Deploy on-chain
node deploy.js --owners "0x...,0x..." --threshold 1 --salt-nonce 12345 --deploy --private-key <hex>
```

## Reusable lib (REST API)

For a REST API or scripts, use the lib directly:

```js
import { loadConfig, predictSafeAddress } from './lib/safe-config.js';
import { deploySafe } from './lib/deploy.js';

const config = loadConfig();
const prediction = await predictSafeAddress(owners, threshold, saltNonce, config);
// prediction.address, prediction.saltNonce, prediction.saltNonceHex, prediction.safeConfig

// Deploy when ready:
const result = await deploySafe(owners, threshold, saltNonce, privateKey, config);
// result.txHash, result.receipt, result.address
```

## References

- [Safe deployment — predict the Safe address](https://docs.safe.global/sdk/protocol-kit/guides/safe-deployment#predict-the-safe-address)
- [safe-deployments](https://github.com/safe-global/safe-deployments)
- [SafeProxyFactory.sol](https://github.com/safe-global/safe-smart-account/blob/main/contracts/proxies/SafeProxyFactory.sol)
