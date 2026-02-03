#!/usr/bin/env node
/**
 * Verify safe_vanity output against the CREATE2 formula and optionally the Safe SDK.
 *
 * Mode 1 - Formula only (verify Rust implementation):
 *   node verify.js --factory <40-hex> --init-code-hash <64-hex> --initializer-hash <64-hex> --salt-nonce <decimal-or-64-hex>
 *   Salt-nonce: decimal (from miner "Salt (dec):") or 64-char hex. Computes address; compare with miner output.
 *
 * Mode 2 - Safe SDK: node verify.js --sdk [--rpc-url <url>] [--owner <0x...>] [--salt-nonce <uint256>]
 */

import { strip0x, computeCreate2Address, toChecksumAddress, saltNonceDecimalToBytes } from './lib/safe-config.js';

function hexToBytes(hex) {
  const h = strip0x(hex);
  if (h.length % 2) throw new Error('Hex length must be even');
  return Buffer.from(h, 'hex');
}

/** Parse salt-nonce: 64 hex chars → 32 bytes; else treat as decimal. */
function saltNonceToBytes(saltNonceStr) {
  const raw = strip0x(saltNonceStr);
  if (raw.length === 64 && /^[0-9a-fA-F]+$/.test(raw)) return Buffer.from(raw, 'hex');
  return saltNonceDecimalToBytes(saltNonceStr);
}

async function runFormulaMode(args) {
  const factory = hexToBytes(args.factory);
  const initCodeHash = hexToBytes(args.initCodeHash);
  const initializerHash = hexToBytes(args.initializerHash);
  const saltNonceBytes = saltNonceToBytes(args.saltNonce);

  if (factory.length !== 20) throw new Error('factory must be 20 bytes (40 hex chars)');
  if (initCodeHash.length !== 32) throw new Error('init_code_hash must be 32 bytes');
  if (initializerHash.length !== 32) throw new Error('initializer_hash must be 32 bytes');
  if (saltNonceBytes.length !== 32) throw new Error('salt_nonce must be 32 bytes (decimal or 64 hex)');

  const address = computeCreate2Address(
    args.factory,
    args.initCodeHash,
    args.initializerHash,
    saltNonceBytes
  );

  console.log('CREATE2 formula result:');
  console.log('Address:     ', toChecksumAddress(address));
  console.log('Address hex: 0x' + address.toString('hex'));
  console.log('Salt nonce:  ', args.saltNonce);
}

async function runSdkMode(args) {
  const { Safe } = await import('@safe-global/protocol-kit');
  const { createPublicClient, http } = await import('viem');
  const { sepolia } = await import('viem/chains');

  const rpcUrl = args.rpcUrl || 'https://ethereum-sepolia-rpc.publicnode.com';
  const owner = args.owner || '0x0000000000000000000000000000000000000001';
  const saltNonce = BigInt(args.saltNonce ?? 0);

  const protocolKit = await Safe.init({
    provider: rpcUrl,
    signer: owner,
    predictedSafe: {
      safeAccountConfig: {
        owners: [owner],
        threshold: 1,
      },
      safeDeploymentConfig: {
        saltNonce: saltNonce.toString(),
      },
    },
  });

  const address = await protocolKit.getAddress();
  console.log('Safe SDK getAddress() result:');
  console.log('Address:    ', address);
  console.log('Salt nonce:', saltNonce.toString());
  console.log('');
  console.log('Use the same factory, init_code_hash, and initializer_hash when running');
  console.log('safe_vanity to mine for this config; the address should match.');
}

function parseArgs() {
  const args = process.argv.slice(2);
  const out = { mode: 'formula', factory: null, initCodeHash: null, initializerHash: null, saltNonce: null, sdk: false, rpcUrl: null, owner: null };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--factory' && args[i + 1]) { out.factory = args[++i]; }
    else if (args[i] === '--init-code-hash' && args[i + 1]) { out.initCodeHash = args[++i]; }
    else if (args[i] === '--initializer-hash' && args[i + 1]) { out.initializerHash = args[++i]; }
    else if (args[i] === '--salt-nonce' && args[i + 1]) { out.saltNonce = args[++i]; }
    else if (args[i] === '--sdk') { out.mode = 'sdk'; }
    else if (args[i] === '--rpc-url' && args[i + 1]) { out.rpcUrl = args[++i]; }
    else if (args[i] === '--owner' && args[i + 1]) { out.owner = args[++i]; }
  }
  return out;
}

async function main() {
  const args = parseArgs();

  if (args.mode === 'sdk') {
    await runSdkMode(args);
    return;
  }

  if (!args.factory || !args.initCodeHash || !args.initializerHash || !args.saltNonce) {
    console.error('Formula mode requires: --factory, --init-code-hash, --initializer-hash, --salt-nonce');
    console.error('Salt-nonce: decimal (from miner) or 64-char hex.');
    console.error('Example: node verify.js --factory <40 hex> --init-code-hash <64 hex> --initializer-hash <64 hex> --salt-nonce 12345');
    console.error('');
    console.error('SDK mode: node verify.js --sdk [--rpc-url <url>] [--owner <0x...>] [--salt-nonce <uint256>]');
    process.exit(1);
  }

  await runFormulaMode(args);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
