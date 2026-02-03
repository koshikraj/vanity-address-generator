/**
 * Shared Safe vanity config: load config file and fetch factory/initCodeHash/initializerHash.
 * Used by run.js, fetch-config.js, deploy.js, and verify.js.
 */

import {
  getProxyFactoryDeployment,
  getSafeSingletonDeployment,
  getSafeL2SingletonDeployment,
  getCompatibilityFallbackHandlerDeployment,
} from '@safe-global/safe-deployments';
import { createPublicClient, http, encodeFunctionData, keccak256 as viemKeccak256 } from 'viem';
import { readFileSync, existsSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import pkg from 'js-sha3';

const { keccak256: jsKeccak256 } = pkg;
const __dirname = dirname(fileURLToPath(import.meta.url));

export const ZERO = '0x0000000000000000000000000000000000000000';

export const DEFAULT_CONFIG = {
  chainId: '11155111',
  rpcUrl: 'https://ethereum-sepolia-rpc.publicnode.com',
  useL2: false,
  safeVersion: '1.4.1',
  fallbackHandler: null,
  minerPath: 'safe_vanity',
  workers: null,
  count: 1,
  reportInterval: 5,
  suffix: null,
  patternType: 'prefix',
  caseSensitive: false,
};

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

const FACTORY_ABI = [
  {
    inputs: [],
    name: 'proxyCreationCode',
    outputs: [{ type: 'bytes' }],
    stateMutability: 'pure',
    type: 'function',
  },
];

export function ensure0x(hex) {
  return hex.startsWith('0x') ? hex : '0x' + hex;
}

export function strip0x(hex) {
  return hex.startsWith('0x') ? hex.slice(2) : hex;
}

/**
 * Load config from file and merge with defaults.
 * Searches: configPath, ./safe-vanity.config.json, ./verify-with-safe-sdk/safe-vanity.config.json, env SAFE_VANITY_CONFIG.
 */
export function loadConfig(configPath) {
  const defaults = { ...DEFAULT_CONFIG };
  const paths = [
    configPath,
    process.env.SAFE_VANITY_CONFIG,
    join(process.cwd(), 'safe-vanity.config.json'),
    join(process.cwd(), 'verify-with-safe-sdk', 'safe-vanity.config.json'),
    join(__dirname, '..', 'safe-vanity.config.json'),
  ].filter(Boolean);

  for (const p of paths) {
    if (existsSync(p)) {
      try {
        const data = readFileSync(p, 'utf8');
        const loaded = JSON.parse(data);
        return { ...defaults, ...loaded };
      } catch (_) {
        // ignore parse errors, try next path
      }
    }
  }
  return defaults;
}

/**
 * Fetch factory, initCodeHash, initializerHash (and related) for the given owners/threshold and config.
 * Config must have: chainId, rpcUrl, useL2, safeVersion; optional fallbackHandler.
 */
export async function fetchSafeConfig(owners, threshold, config) {
  const chainId = String(config.chainId ?? DEFAULT_CONFIG.chainId);
  const filter = { network: chainId, released: true };
  if (config.safeVersion) filter.version = config.safeVersion;

  const factoryDeployment = getProxyFactoryDeployment(filter);
  if (!factoryDeployment) throw new Error(`No SafeProxyFactory found for chain ${chainId}`);

  const singletonDeployment = config.useL2
    ? getSafeL2SingletonDeployment(filter)
    : getSafeSingletonDeployment(filter);
  if (!singletonDeployment) throw new Error(`No Safe${config.useL2 ? 'L2' : ''} singleton for chain ${chainId}`);

  const factoryAddress = factoryDeployment.networkAddresses[chainId];
  const singletonAddress = singletonDeployment.networkAddresses[chainId];
  if (!factoryAddress || !singletonAddress) throw new Error(`Missing addresses for chain ${chainId}`);

  let fallbackHandler = config.fallbackHandler ?? null;
  if (!fallbackHandler) {
    const fbDeployment = getCompatibilityFallbackHandlerDeployment(filter);
    if (fbDeployment?.networkAddresses[chainId]) fallbackHandler = fbDeployment.networkAddresses[chainId];
    else fallbackHandler = ZERO;
  }

  const publicClient = createPublicClient({ transport: http(config.rpcUrl) });
  const creationCodeHex = await publicClient.readContract({
    address: factoryAddress,
    abi: FACTORY_ABI,
    functionName: 'proxyCreationCode',
    args: [],
  });
  const creationCode = strip0x(creationCodeHex);
  const singletonPadded = strip0x(ensure0x(singletonAddress)).padStart(64, '0');
  const deploymentDataHex = '0x' + creationCode + singletonPadded;
  const initCodeHashHex = viemKeccak256(deploymentDataHex);
  const initCodeHash = strip0x(initCodeHashHex);

  const initializerCalldata = encodeFunctionData({
    abi: SAFE_SETUP_ABI,
    functionName: 'setup',
    args: [
      owners.map(ensure0x),
      BigInt(threshold),
      ZERO,
      '0x',
      ensure0x(fallbackHandler),
      ZERO,
      BigInt(0),
      ZERO,
    ],
  });
  const initializerHashHex = viemKeccak256(initializerCalldata);
  const initializerHash = strip0x(initializerHashHex);

  return {
    factory: factoryAddress,
    initCodeHash: initCodeHashHex.startsWith('0x') ? initCodeHashHex : '0x' + initCodeHash,
    initializerHash: initializerHashHex.startsWith('0x') ? initializerHashHex : '0x' + initializerHash,
    chainId,
    rpcUrl: config.rpcUrl,
    fallbackHandler: ensure0x(fallbackHandler),
    useL2: !!config.useL2,
    safeVersion: singletonDeployment.version,
    singletonAddress,
    owners,
    threshold,
  };
}

/** Convert decimal salt string to 32-byte big-endian Buffer (same as Rust miner). */
export function saltNonceDecimalToBytes(decStr) {
  const hex = BigInt(decStr).toString(16).padStart(64, '0');
  return Buffer.from(hex, 'hex');
}

/** Compute CREATE2 address from raw params (same formula as Rust miner and verify.js). */
export function computeCreate2Address(factory, initCodeHash, initializerHash, saltNonceBytes) {
  const factoryBytes = Buffer.from(strip0x(factory), 'hex');
  const initCodeHashBytes = Buffer.from(strip0x(initCodeHash), 'hex');
  const initializerHashBytes = Buffer.from(strip0x(initializerHash), 'hex');
  const saltPreimage = Buffer.concat([initializerHashBytes, saltNonceBytes]);
  const salt = Buffer.from(jsKeccak256.arrayBuffer(saltPreimage));
  const preimage = Buffer.concat([
    Buffer.from([0xff]),
    factoryBytes,
    salt,
    initCodeHashBytes,
  ]);
  const hash = Buffer.from(jsKeccak256.arrayBuffer(preimage));
  return hash.slice(12, 32);
}

/** EIP-55 checksum for 20-byte address. */
export function toChecksumAddress(addressBytes) {
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
