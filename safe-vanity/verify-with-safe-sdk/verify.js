#!/usr/bin/env node
/**
 * Verify safe_vanity output against the CREATE2 formula and optionally the Safe SDK.
 *
 * Mode 1 - Formula only (verify Rust implementation):
 *   node verify.js --factory <20-byte-hex> --init-code-hash <32-byte-hex> \
 *     --initializer-hash <32-byte-hex> --salt-nonce <32-byte-hex>
 *   Computes Safe address with CREATE2 formula and prints it. Compare with safe_vanity output.
 *
 * Mode 2 - Safe SDK (verify against actual SDK prediction):
 *   node verify.js --sdk --rpc-url <url> --owner <0x...> --salt-nonce <uint256>
 *   Uses @safe-global/protocol-kit to predict Safe address for the given config.
 *   Compare the printed address with safe_vanity when using the same factory/init_code_hash/initializer_hash.
 */

import pkg from 'js-sha3';
const { keccak256: jsKeccak256 } = pkg;

function strip0x(hex) {
  return hex.startsWith('0x') ? hex.slice(2) : hex;
}

function hexToBytes(hex) {
  const h = strip0x(hex);
  if (h.length % 2) throw new Error('Hex length must be even');
  return Buffer.from(h, 'hex');
}

/**
 * Safe CREATE2 salt: keccak256(initializerHash || saltNonce) — 64 bytes in, 32 bytes out.
 */
function safeSalt(initializerHashBytes, saltNonceBytes) {
  const preimage = Buffer.concat([initializerHashBytes, saltNonceBytes]);
  return Buffer.from(jsKeccak256.arrayBuffer(preimage));
}

/**
 * Safe proxy address: keccak256(0xff || factory || salt || initCodeHash)[12:32].
 */
function safeAddress(factoryBytes, initCodeHashBytes, saltBytes) {
  const preimage = Buffer.concat([
    Buffer.from([0xff]),
    factoryBytes,
    saltBytes,
    initCodeHashBytes,
  ]);
  if (preimage.length !== 85) throw new Error('Preimage must be 85 bytes');
  const hash = Buffer.from(jsKeccak256.arrayBuffer(preimage));
  return hash.slice(12, 32);
}

function toChecksumAddress(addressBytes) {
  const hex = addressBytes.toString('hex').toLowerCase();
  const hash = Buffer.from(jsKeccak256.arrayBuffer(Buffer.from(hex, 'utf8')));
  let out = '0x';
  for (let i = 0; i < 40; i++) {
    const byteIndex = i >> 1;
    const nibble = i % 2 === 0 ? (hash[byteIndex] >> 4) : (hash[byteIndex] & 0x0f);
    const c = hex[i];
    out += nibble >= 8 ? c.toUpperCase() : c;
  }
  return out;
}

async function runFormulaMode(args) {
  const factory = hexToBytes(args.factory);
  const initCodeHash = hexToBytes(args.initCodeHash);
  const initializerHash = hexToBytes(args.initializerHash);
  const saltNonce = hexToBytes(args.saltNonce);

  if (factory.length !== 20) throw new Error('factory must be 20 bytes (40 hex chars)');
  if (initCodeHash.length !== 32) throw new Error('init_code_hash must be 32 bytes');
  if (initializerHash.length !== 32) throw new Error('initializer_hash must be 32 bytes');
  if (saltNonce.length !== 32) throw new Error('salt_nonce must be 32 bytes (64 hex chars)');

  const salt = safeSalt(initializerHash, saltNonce);
  const address = safeAddress(factory, initCodeHash, salt);

  console.log('CREATE2 formula result:');
  console.log('Address:     ', toChecksumAddress(address));
  console.log('Address hex: 0x' + address.toString('hex'));
  console.log('Salt nonce:  0x' + saltNonce.toString('hex'));
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
    console.error('Example: node verify.js --factory <40 hex> --init-code-hash <64 hex> --initializer-hash <64 hex> --salt-nonce <64 hex>');
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
