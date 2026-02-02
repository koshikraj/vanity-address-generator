# Safe Smart Account Vanity Address Design

Predict a Safe (Gnosis Safe) proxy address by varying **saltNonce** until the CREATE2-derived address matches a desired pattern (prefix/suffix/contains). Same idea as EOA vanity, but the address comes from CREATE2, so **no elliptic curve** — only Keccak-256 and packing. Very GPU-friendly.

References:
- [Safe deployment guide – predict the Safe address](https://docs.safe.global/sdk/protocol-kit/guides/safe-deployment#predict-the-safe-address)
- [Protocol Kit init – predictedSafe](https://docs.safe.global/reference-sdk-protocol-kit/initialization/init#predictedsafe-optional)
- [SafeProxyFactory.sol](https://github.com/safe-global/safe-smart-account/blob/main/contracts/proxies/SafeProxyFactory.sol)

---

## 1. How Safe proxy address is computed

From **SafeProxyFactory.sol**:

```solidity
// createProxyWithNonce(singleton, initializer, saltNonce)
bytes32 salt = keccak256(abi.encodePacked(keccak256(initializer), saltNonce));
// deployProxy: CREATE2 with deploymentData = creationCode || uint256(uint160(singleton))
proxy := create2(0x0, add(0x20, deploymentData), mload(deploymentData), salt)
```

EIP-1014 CREATE2:

`address = keccak256(0xff ++ factory (20) ++ salt (32) ++ keccak256(initCode) (32))[12:32]`

Where:

- **factory** = SafeProxyFactory address (20 bytes).
- **salt** = `keccak256(keccak256(initializer) || saltNonce)` (32 bytes). So you need the **initializer hash** (32 bytes) and you vary **saltNonce** (uint256).
- **initCode** = SafeProxy creation code (bytecode) concatenated with `uint256(uint160(singleton))`. So **initCodeHash** = `keccak256(creationCode || singleton_uint256)` (32 bytes). This is fixed for a given Safe version and singleton.

So for **prediction / mining** we have:

| Input | Size | Source |
|-------|------|--------|
| factory | 20 bytes | Chain + Safe deployment (e.g. safe-deployments) |
| initCodeHash | 32 bytes | `keccak256(proxy_creation_code \|\| singleton_uint256)` |
| initializerHash | 32 bytes | `keccak256(initializer)` — from owners, threshold, etc. |
| saltNonce | 32 bytes (uint256) | **Variable** — we iterate this to get a vanity address |

Per attempt:

1. `salt = keccak256(initializerHash || saltNonce)`  — 64-byte input, one Keccak-256.
2. `preimage = 0xff || factory || salt || initCodeHash`  — 1 + 20 + 32 + 32 = **85 bytes**.
3. `address = keccak256(preimage)[12..32]`  — 20-byte address.

So **2 Keccak-256 hashes per attempt**, no EC, no big-int. Same pattern as generic CREATE2 salt mining.

---

## 2. CPU vs GPU vs Rust — best approach

| Aspect | Recommendation | Reason |
|--------|----------------|--------|
| **Language** | **Rust** | Same stack as eth-vanity: tiny_keccak, crossbeam, optional OpenCL; easy to reuse matcher, config, worker pool. |
| **CPU** | **Yes** | Simple loop: increment or random `saltNonce`, compute salt → address, pattern match. Good for correctness and small runs. |
| **GPU** | **Yes, high priority** | Only 2 Keccak-256 per attempt; no EC. Throughput can be 10–100× higher than EOA vanity on the same GPU. |
| **Reuse** | **Extend eth-vanity** | Same CLI style, same `Pattern`/matcher, same worker pool and stats; add a “mode” (EOA vs Safe) and Safe-specific args. |

So: **implement in Rust, with both CPU and GPU workers**, reusing eth-vanity’s structure and adding a Safe (CREATE2) mode.

---

## 3. What to implement

### 3.1 Config / CLI (e.g. in `config.rs` or a Safe-specific module)

- **Mode**: EOA vanity (current) vs **Safe vanity** (CREATE2 salt mining).
- Safe-specific args (when mode = Safe):
  - `--factory <hex>` — factory address (20 bytes).
  - `--init-code-hash <hex>` — 32-byte keccak256(creationCode || singleton). Optional if you derive from `--singleton` + known creation code.
  - `--initializer-hash <hex>` — 32-byte keccak256(initializer). User can pass precomputed hash (e.g. from Safe SDK’s `getAddress()` flow).
  - Or `--initializer <hex>` and hash it in Rust (more convenient; then you don’t need SDK to precompute hash).

Same as now: `-p` / `--pattern`, `-t` prefix/suffix/contains, `-n` count, `--gpu`, `--gpu-work-size`, `--workers`.

### 3.2 CPU worker (Safe mode)

- Loop:
  - Next `saltNonce` (e.g. random 256-bit or `base + counter`).
  - `salt = keccak256(initializer_hash || saltNonce)` (64 bytes).
  - `preimage = 0xff || factory || salt || initCodeHash` (85 bytes).
  - `address = keccak256(preimage)[12..32]`.
  - If `pattern.matches(address)` → send result (saltNonce, address), optionally verify with same formula.
- Reuse existing `WorkerStats`, channel, and stop flag.

### 3.3 GPU kernel (Safe mode)

- **Inputs (read-only):** factory (20), initCodeHash (32), initializerHash (32), pattern config.
- **Outputs:** matching saltNonces (and addresses if you want).
- **Per work item:**  
  `saltNonce = base_salt + global_id` (or similar). Then:
  1. Build 64-byte buffer: initializerHash || saltNonce (big-endian).
  2. Keccak-256 → salt (32 bytes).
  3. Build 85-byte buffer: 0xff || factory || salt || initCodeHash.
  4. Keccak-256 → full hash; address = hash[12..32].
  5. Pattern match on address (reuse existing nibble matching).
- Reuse existing Keccak and pattern-matching code; add a **keccak256 variant for 64-byte and 85-byte inputs** (or one generic “absorb N bytes then squeeze 32”) so you don’t duplicate the permutation. Same as in CLAUDE.md’s CREATE2 section.

### 3.4 Result type

- For Safe mode, result = `{ salt_nonce: [u8; 32], address: 20-byte }`. No private key. Display salt in hex; user passes it to Safe SDK as `saltNonce` (uint256) when deploying.

### 3.5 Getting factory / initCodeHash / initializer hash

- **factory**: From [safe-deployments](https://github.com/safe-global/safe-deployments) for the chain (e.g. mainnet, Sepolia).
- **initCodeHash**: Either:
  - Precomputed and passed as `--init-code-hash`, or
  - Computed in Rust from SafeProxy creation code (fixed per Safe version) + singleton address. That requires embedding or fetching the creation code bytecode.
- **initializerHash**: `keccak256(initializer)` where `initializer` is the ABI-encoded setup call (e.g. `setup(owners[], threshold, ...)`). User can get this from the Safe SDK (e.g. same payload used for `getAddress()`) or encode it in Rust if you add ABI encoding.

So the **minimal** implementation is: user passes `--factory`, `--init-code-hash`, `--initializer-hash` (all hex). No dependency on Safe SDK in Rust; SDK is only used to prepare the deployment with the mined `saltNonce`.

---

## 4. Summary

| Question | Answer |
|----------|--------|
| **What do we vary?** | **saltNonce** (uint256). Same as “Predict the Safe address” with different nonces. |
| **Formula** | `salt = keccak256(initializerHash \|\| saltNonce)`; `address = keccak256(0xff \|\| factory \|\| salt \|\| initCodeHash)[12:32]`. |
| **CPU** | Yes; simple Keccak + pattern match loop. |
| **GPU** | Yes; very effective (2 Keccak per attempt, no EC). |
| **Rust** | Best option here; reuse eth-vanity and keep one codebase. |
| **Where** | Extend eth-vanity with a Safe (CREATE2) mode and optional new kernel/entry point. |

This gives you a Safe-equivalent of eth-vanity: predict the Safe address by scanning saltNonces until the CREATE2 address matches your pattern, with high throughput on both CPU and GPU.
