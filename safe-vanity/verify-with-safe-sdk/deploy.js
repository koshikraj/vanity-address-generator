#!/usr/bin/env node
/**
 * Deploy a Safe using a mined saltNonce from safe_vanity.
 *
 * Workflow: fetch-config.js -> safe_vanity (mine) -> verify.js -> deploy.js
 *
 * This script resolves the same config as fetch-config.js (factory, singleton,
 * fallback handler from @safe-global/safe-deployments), computes the CREATE2
 * address with the raw formula, and deploys by calling createProxyWithNonce
 * on the factory contract directly — no SafeFactory SDK indirection.
 *
 * Usage:
 *   # Predict only (dry run):
 *   node deploy.js --chain-id 11155111 --owners 0x... --threshold 1 --salt-nonce 12345 --rpc-url <url>
 *
 *   # Deploy on-chain:
 *   node deploy.js --chain-id 11155111 --owners 0x... --threshold 1 --salt-nonce 12345 \
 *     --rpc-url <url> --private-key <hex> --deploy
 */

import {
  getProxyFactoryDeployment,
  getSafeSingletonDeployment,
  getSafeL2SingletonDeployment,
  getCompatibilityFallbackHandlerDeployment,
} from '@safe-global/safe-deployments';
import {
  createPublicClient,
  createWalletClient,
  http,
  encodeFunctionData,
  keccak256 as viemKeccak256,
} from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import pkg from 'js-sha3';
const { keccak256: jsKeccak256 } = pkg;

const ZERO = '0x0000000000000000000000000000000000000000';

// Safe.setup(address[],uint256,address,bytes,address,address,uint256,address)
const SAFE_SETUP_ABI = [
  {
    inputs: [
      { name: '_owners', type: 'address[]' },
      { name: '_threshold', type: 'uint256' },
      { name: 'to', type: 'address' },
      { name: 'data', type: 'bytes' },
      { name: 'fallbackHandler', type: 'address' },
      { name: 'paymentToken', type: 'address' },
      { name: 'payment', type: 'uint256' },
      { name: 'paymentReceiver', type: 'address' },
    ],
    name: 'setup',
    type: 'function',
  },
];

// SafeProxyFactory ABI (the two functions we need)
const FACTORY_ABI = [
  {
    inputs: [],
    name: 'proxyCreationCode',
    outputs: [{ type: 'bytes' }],
    stateMutability: 'pure',
    type: 'function',
  },
  {
    inputs: [
      { name: '_singleton', type: 'address' },
      { name: 'initializer', type: 'bytes' },
      { name: 'saltNonce', type: 'uint256' },
    ],
    name: 'createProxyWithNonce',
    outputs: [{ name: 'proxy', type: 'address' }],
    stateMutability: 'nonpayable',
    type: 'function',
  },
];

function ensure0x(hex) {
  return hex.startsWith('0x') ? hex : '0x' + hex;
}

function strip0x(hex) {
  return hex.startsWith('0x') ? hex.slice(2) : hex;
}

/**
 * Compute CREATE2 address from raw parameters (same formula as verify.js / Rust miner).
 */
function computeCreate2Address(factory, initCodeHash, initializerHash, saltNonceBytes) {
  const factoryBytes = Buffer.from(strip0x(factory), 'hex');
  const initCodeHashBytes = Buffer.from(strip0x(initCodeHash), 'hex');
  const initializerHashBytes = Buffer.from(strip0x(initializerHash), 'hex');

  // salt = keccak256(initializerHash || saltNonce)
  const saltPreimage = Buffer.concat([initializerHashBytes, saltNonceBytes]);
  const salt = Buffer.from(jsKeccak256.arrayBuffer(saltPreimage));

  // address = keccak256(0xff || factory || salt || initCodeHash)[12:32]
  const preimage = Buffer.concat([
    Buffer.from([0xff]),
    factoryBytes,
    salt,
    initCodeHashBytes,
  ]);
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

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    chainId: null,
    owners: null,
    threshold: null,
    saltNonce: null,
    rpcUrl: null,
    privateKey: null,
    deploy: false,
    useL2: false,
    fallbackHandler: null,
    safeVersion: null,
  };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--chain-id' && args[i + 1]) out.chainId = args[++i];
    else if (args[i] === '--owners' && args[i + 1]) out.owners = args[++i].split(',').map(a => a.trim());
    else if (args[i] === '--threshold' && args[i + 1]) out.threshold = parseInt(args[++i], 10);
    else if (args[i] === '--salt-nonce' && args[i + 1]) out.saltNonce = args[++i];
    else if (args[i] === '--rpc-url' && args[i + 1]) out.rpcUrl = args[++i];
    else if (args[i] === '--private-key' && args[i + 1]) out.privateKey = args[++i];
    else if (args[i] === '--deploy') out.deploy = true;
    else if (args[i] === '--l2') out.useL2 = true;
    else if (args[i] === '--fallback-handler' && args[i + 1]) out.fallbackHandler = args[++i];
    else if (args[i] === '--safe-version' && args[i + 1]) out.safeVersion = args[++i];
  }
  return out;
}

async function main() {
  const args = parseArgs();

  if (!args.chainId || !args.owners || !args.threshold || !args.saltNonce || !args.rpcUrl) {
    console.error('Usage: node deploy.js --chain-id <id> --owners <0x...> --threshold <n> --salt-nonce <decimal> --rpc-url <url> [--deploy --private-key <hex>]');
    console.error('');
    console.error('Required:');
    console.error('  --chain-id <id>        Chain ID');
    console.error('  --owners <addrs>       Comma-separated owner addresses');
    console.error('  --threshold <n>        Required signatures');
    console.error('  --salt-nonce <dec>     Salt nonce (decimal, from safe_vanity output)');
    console.error('  --rpc-url <url>        RPC endpoint');
    console.error('');
    console.error('Optional:');
    console.error('  --deploy               Actually deploy (without this, only predicts address)');
    console.error('  --private-key <hex>    Deployer private key (required with --deploy)');
    console.error('  --l2                   Use SafeL2 singleton');
    console.error('  --fallback-handler <addr>  Override fallback handler');
    console.error('  --safe-version <ver>       Filter Safe version (e.g. 1.3.0, 1.4.1)');
    process.exit(1);
  }

  if (args.deploy && !args.privateKey) {
    console.error('Error: --private-key is required when using --deploy');
    process.exit(1);
  }

  // ── Resolve the same config as fetch-config.js ──
  const chainId = String(args.chainId);
  const filter = { network: chainId, released: true };
  if (args.safeVersion) filter.version = args.safeVersion;

  const factoryDeployment = getProxyFactoryDeployment(filter);
  if (!factoryDeployment) { console.error('No SafeProxyFactory found for chain', chainId); process.exit(1); }

  const singletonDeployment = args.useL2
    ? getSafeL2SingletonDeployment(filter)
    : getSafeSingletonDeployment(filter);
  if (!singletonDeployment) { console.error('No singleton found for chain', chainId); process.exit(1); }

  const factoryAddress = factoryDeployment.networkAddresses[chainId];
  const singletonAddress = singletonDeployment.networkAddresses[chainId];
  if (!factoryAddress || !singletonAddress) { console.error('Missing addresses for chain', chainId); process.exit(1); }

  // Resolve fallback handler (same logic as fetch-config.js)
  let fallbackHandler = args.fallbackHandler;
  if (!fallbackHandler) {
    const fbDeployment = getCompatibilityFallbackHandlerDeployment(filter);
    if (fbDeployment && fbDeployment.networkAddresses[chainId]) {
      fallbackHandler = fbDeployment.networkAddresses[chainId];
    } else {
      fallbackHandler = ZERO;
    }
  }

  const publicClient = createPublicClient({ transport: http(args.rpcUrl) });

  // Fetch init code hash (same as fetch-config.js)
  const creationCodeHex = await publicClient.readContract({
    address: factoryAddress,
    abi: FACTORY_ABI,
    functionName: 'proxyCreationCode',
    args: [],
  });
  const creationCode = strip0x(creationCodeHex);
  const singletonPadded = strip0x(singletonAddress).padStart(64, '0');
  const deploymentDataHex = '0x' + creationCode + singletonPadded;
  const initCodeHash = viemKeccak256(deploymentDataHex);

  // Encode initializer calldata (same as fetch-config.js)
  const initializerCalldata = encodeFunctionData({
    abi: SAFE_SETUP_ABI,
    functionName: 'setup',
    args: [
      args.owners.map(ensure0x),
      BigInt(args.threshold),
      ZERO,
      '0x',
      ensure0x(fallbackHandler),
      ZERO,
      BigInt(0),
      ZERO,
    ],
  });
  const initializerHash = viemKeccak256(initializerCalldata);

  // Convert decimal saltNonce to 32-byte big-endian
  const saltNonceBigInt = BigInt(args.saltNonce);
  const saltNonceHex = saltNonceBigInt.toString(16).padStart(64, '0');
  const saltNonceBytes = Buffer.from(saltNonceHex, 'hex');

  // Compute CREATE2 address using raw formula (same as Rust miner + verify.js)
  const formulaAddress = computeCreate2Address(factoryAddress, initCodeHash, initializerHash, saltNonceBytes);
  const formulaChecksum = toChecksumAddress(formulaAddress);

  console.log('Safe Deployment');
  console.log('===============');
  console.log('Chain ID:          ', chainId);
  console.log('Owners:            ', args.owners.join(', '));
  console.log('Threshold:         ', args.threshold);
  console.log('Salt nonce (dec):  ', args.saltNonce);
  console.log('Salt nonce (hex):   0x' + saltNonceHex);
  console.log('Factory:           ', factoryAddress);
  console.log('Singleton:         ', singletonAddress);
  console.log('Fallback handler:  ', fallbackHandler);
  console.log('Init code hash:    ', initCodeHash);
  console.log('Initializer hash:  ', initializerHash);
  console.log('');
  console.log('Predicted address: ', formulaChecksum);

  if (!args.deploy) {
    console.log('');
    console.log('Dry run complete. Use --deploy --private-key <key> to deploy on-chain.');
    return;
  }

  // ── Deploy by calling createProxyWithNonce directly ──
  // This calls the exact factory contract we resolved, with the exact singleton
  // and initializer we computed — guaranteed to produce the formula address.
  console.log('');
  console.log('Deploying Safe...');

  const account = privateKeyToAccount(ensure0x(args.privateKey));
  const walletClient = createWalletClient({
    account,
    transport: http(args.rpcUrl),
  });

  const txHash = await walletClient.writeContract({
    address: factoryAddress,
    abi: FACTORY_ABI,
    functionName: 'createProxyWithNonce',
    args: [
      singletonAddress,
      initializerCalldata,
      saltNonceBigInt,
    ],
  });

  console.log('Transaction hash:  ', txHash);
  console.log('Waiting for confirmation...');

  const receipt = await publicClient.waitForTransactionReceipt({ hash: txHash });
  console.log('Confirmed in block:', receipt.blockNumber.toString());
  console.log('Status:            ', receipt.status === 'success' ? 'SUCCESS' : 'FAILED');

  if (receipt.status !== 'success') {
    console.error('Deployment transaction failed!');
    process.exit(1);
  }

  // Verify the proxy was deployed at the expected address
  const code = await publicClient.getCode({ address: formulaChecksum });
  if (code && code !== '0x') {
    console.log('');
    console.log('Safe deployed at:  ', formulaChecksum);
    console.log('Verified: contract code exists at predicted address.');
  } else {
    console.error('');
    console.error('WARNING: No code found at predicted address', formulaChecksum);
    console.error('The deployment may have created the proxy at a different address.');
    console.error('Check the transaction logs for the ProxyCreation event.');
  }
}

main().catch((err) => {
  console.error('Error:', err.message || err);
  process.exit(1);
});
