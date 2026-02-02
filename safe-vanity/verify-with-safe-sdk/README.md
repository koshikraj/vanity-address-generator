# Safe Vanity - JS Tools

Fetch config, verify results, and deploy Safes mined by `safe_vanity`.

## Install

```bash
npm install
```

## Scripts

### 1. Fetch Config

Resolves factory, init_code_hash, and initializer_hash from `@safe-global/safe-deployments` + on-chain RPC call. Auto-detects the CompatibilityFallbackHandler.

```bash
node fetch-config.js \
  --chain-id 11155111 \
  --owners "0xOwner1,0xOwner2" \
  --threshold 1 \
  --rpc-url https://ethereum-sepolia-rpc.publicnode.com
```

Options: `--fallback-handler <addr>`, `--l2`, `--safe-version <ver>`

### 2. Verify (Formula)

Cross-check safe_vanity output against the CREATE2 formula in JS:

```bash
node verify.js \
  --factory <40-hex> \
  --init-code-hash <64-hex> \
  --initializer-hash <64-hex> \
  --salt-nonce <64-hex>
```

### 3. Verify (Safe SDK)

Verify against `@safe-global/protocol-kit` prediction:

```bash
node verify.js --sdk --rpc-url https://... --owner 0x... --salt-nonce <decimal>
```

### 4. Deploy

Deploy the mined Safe on-chain:

```bash
# Dry run (predict only):
node deploy.js --chain-id 11155111 --owners 0x... --threshold 1 \
  --salt-nonce <decimal> --rpc-url https://...

# Actual deployment:
node deploy.js --chain-id 11155111 --owners 0x... --threshold 1 \
  --salt-nonce <decimal> --rpc-url https://... \
  --private-key <hex> --deploy
```

## References

- [Safe deployment — predict the Safe address](https://docs.safe.global/sdk/protocol-kit/guides/safe-deployment#predict-the-safe-address)
- [Protocol Kit init — predictedSafe](https://docs.safe.global/reference-sdk-protocol-kit/initialization/init#predictedsafe-optional)
- [safe-deployments](https://github.com/safe-global/safe-deployments)
- [SafeProxyFactory.sol](https://github.com/safe-global/safe-smart-account/blob/main/contracts/proxies/SafeProxyFactory.sol)
