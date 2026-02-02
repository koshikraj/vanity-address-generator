#!/usr/bin/env node
/**
 * Fetch factory, init_code_hash, and initializer_hash from Safe config for use with safe_vanity.
 *
 * Uses @safe-global/safe-deployments for factory/singleton/fallback-handler addresses per chain
 * and (with RPC) the factory's proxyCreationCode() then keccak256(creationCode||singleton). Encodes the Safe setup()
 * initializer and hashes it for initializer_hash.
 *
 * Refs:
 * - https://docs.safe.global/sdk/protocol-kit/guides/safe-deployment#predict-the-safe-address
 * - https://docs.safe.global/reference-sdk-protocol-kit/initialization/init#predictedsafe-optional
 * - https://github.com/safe-global/safe-deployments
 * - https://github.com/safe-global/safe-smart-account/blob/main/contracts/proxies/SafeProxyFactory.sol
 *
 * Usage:
 *   node fetch-config.js --chain-id 11155111 --owners 0x...,0x... --threshold 1 --rpc-url <url>
 *   node fetch-config.js --chain-id 1 --owners 0x... --threshold 1 --rpc-url <url> [--fallback-handler 0x...]
 */

import {
  getProxyFactoryDeployment,
  getSafeSingletonDeployment,
  getSafeL2SingletonDeployment,
  getCompatibilityFallbackHandlerDeployment,
} from '@safe-global/safe-deployments';
import { createPublicClient, http, encodeFunctionData, keccak256 as viemKeccak256 } from 'viem';

const ZERO = '0x0000000000000000000000000000000000000000';

const CHAIN_NAMES = {
  '1': 'Ethereum Mainnet',
  '10': 'Optimism',
  '56': 'BNB Chain',
  '100': 'Gnosis',
  '137': 'Polygon',
  '8453': 'Base',
  '42161': 'Arbitrum One',
  '43114': 'Avalanche',
  '11155111': 'Sepolia',
  '84532': 'Base Sepolia',
};

// Safe setup(address[],uint256,address,bytes,address,address,uint256,address)
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

// SafeProxyFactory.proxyCreationCode() returns bytes (creation code only).
// deploymentData = creationCode || uint256(uint160(singleton)); initCodeHash = keccak256(deploymentData).
const FACTORY_ABI = [
  {
    inputs: [],
    name: 'proxyCreationCode',
    outputs: [{ type: 'bytes' }],
    stateMutability: 'pure',
    type: 'function',
  },
];

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    chainId: null,
    owners: null,
    threshold: null,
    rpcUrl: null,
    fallbackHandler: null, // null = auto-detect from safe-deployments
    useL2: false,
    safeVersion: null,
  };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--chain-id' && args[i + 1]) {
      out.chainId = args[++i];
    } else if (args[i] === '--owners' && args[i + 1]) {
      out.owners = args[++i].split(',').map((a) => a.trim());
    } else if (args[i] === '--threshold' && args[i + 1]) {
      out.threshold = parseInt(args[++i], 10);
    } else if (args[i] === '--rpc-url' && args[i + 1]) {
      out.rpcUrl = args[++i];
    } else if (args[i] === '--fallback-handler' && args[i + 1]) {
      out.fallbackHandler = args[++i];
    } else if (args[i] === '--l2') {
      out.useL2 = true;
    } else if (args[i] === '--safe-version' && args[i + 1]) {
      out.safeVersion = args[++i];
    }
  }
  return out;
}

function ensure0x(hex) {
  return hex.startsWith('0x') ? hex : '0x' + hex;
}

/**
 * Compute initializer_hash = keccak256(encoded setup calldata).
 * Same as SafeProxyFactory: salt = keccak256(abi.encodePacked(keccak256(initializer), saltNonce)).
 */
function computeInitializerHash(owners, threshold, fallbackHandler) {
  const initializerCalldata = encodeFunctionData({
    abi: SAFE_SETUP_ABI,
    functionName: 'setup',
    args: [
      owners.map(ensure0x),
      BigInt(threshold),
      ZERO,           // to
      '0x',           // data
      ensure0x(fallbackHandler),
      ZERO,           // paymentToken
      BigInt(0),      // payment
      ZERO,           // paymentReceiver
    ],
  });
  return viemKeccak256(initializerCalldata);
}

async function main() {
  const args = parseArgs();

  if (!args.chainId || !args.owners || !args.threshold) {
    console.error('Usage: node fetch-config.js --chain-id <id> --owners <0x...,0x...> --threshold <n> --rpc-url <url>');
    console.error('');
    console.error('Required:');
    console.error('  --chain-id <id>         Chain ID (e.g. 1 = mainnet, 11155111 = Sepolia)');
    console.error('  --owners <addrs>        Comma-separated owner addresses (0x...)');
    console.error('  --threshold <n>         Number of required signatures');
    console.error('  --rpc-url <url>         RPC URL (to call factory.proxyCreationCode)');
    console.error('');
    console.error('Optional:');
    console.error('  --fallback-handler <addr>  Override fallback handler (default: auto-detect from safe-deployments)');
    console.error('  --l2                       Use SafeL2 singleton');
    console.error('  --safe-version <ver>       Filter by Safe version (e.g. 1.3.0, 1.4.1)');
    process.exit(1);
  }

  if (!args.rpcUrl) {
    console.error('Error: --rpc-url is required to fetch init_code_hash from the factory contract.');
    process.exit(1);
  }

  const chainId = String(args.chainId);
  const chainName = CHAIN_NAMES[chainId] || `Chain ${chainId}`;
  const filter = { network: chainId, released: true };
  if (args.safeVersion) filter.version = args.safeVersion;

  // Resolve factory
  const factoryDeployment = getProxyFactoryDeployment(filter);
  if (!factoryDeployment) {
    console.error(`No SafeProxyFactory deployment found for ${chainName} (chain ${chainId})`);
    process.exit(1);
  }

  // Resolve singleton
  const singletonDeployment = args.useL2
    ? getSafeL2SingletonDeployment(filter)
    : getSafeSingletonDeployment(filter);
  if (!singletonDeployment) {
    console.error(`No Safe${args.useL2 ? 'L2' : ''} singleton deployment found for ${chainName} (chain ${chainId})`);
    process.exit(1);
  }

  const factoryAddress = factoryDeployment.networkAddresses[chainId];
  const singletonAddress = singletonDeployment.networkAddresses[chainId];

  if (!factoryAddress || !singletonAddress) {
    console.error(`Factory or singleton address not found for ${chainName} (chain ${chainId})`);
    process.exit(1);
  }

  // Resolve fallback handler: auto-detect from safe-deployments if not specified
  let fallbackHandler = args.fallbackHandler;
  let fallbackSource = 'user-provided';
  if (!fallbackHandler) {
    const fbDeployment = getCompatibilityFallbackHandlerDeployment(filter);
    if (fbDeployment && fbDeployment.networkAddresses[chainId]) {
      fallbackHandler = fbDeployment.networkAddresses[chainId];
      fallbackSource = 'safe-deployments (auto-detected)';
    } else {
      fallbackHandler = ZERO;
      fallbackSource = 'zero (no deployment found)';
    }
  }

  // Fetch creation code from factory, then initCodeHash = keccak256(creationCode || uint256(singleton))
  const publicClient = createPublicClient({
    transport: http(args.rpcUrl),
  });

  const creationCodeHex = await publicClient.readContract({
    address: factoryAddress,
    abi: FACTORY_ABI,
    functionName: 'proxyCreationCode',
    args: [],
  });
  const creationCode = creationCodeHex.startsWith('0x') ? creationCodeHex.slice(2) : creationCodeHex;
  // deploymentData = abi.encodePacked(creationCode, uint256(uint160(singleton))) → creationCode + 32 bytes (singleton left-padded)
  const singletonPadded = (ensure0x(singletonAddress).slice(2).padStart(64, '0'));
  const deploymentDataHex = '0x' + creationCode + singletonPadded;
  const initCodeHashHex = viemKeccak256(deploymentDataHex);
  const initCodeHash = initCodeHashHex.startsWith('0x') ? initCodeHashHex.slice(2) : initCodeHashHex;
  if (initCodeHash.length !== 64) {
    console.error('Unexpected init_code_hash length:', initCodeHash.length);
    process.exit(1);
  }

  // Compute initializer hash
  const initializerHashHex = computeInitializerHash(args.owners, args.threshold, fallbackHandler);
  const initializerHash = initializerHashHex.startsWith('0x') ? initializerHashHex.slice(2) : initializerHashHex;

  // Output
  console.log(`# Safe vanity config for ${chainName} (chain ${chainId})`);
  console.log(`# Safe version:      ${singletonDeployment.version}`);
  console.log(`# Factory:           ${factoryAddress}`);
  console.log(`# Singleton:         ${singletonAddress} ${args.useL2 ? '(L2)' : ''}`);
  console.log(`# Fallback handler:  ${fallbackHandler} (${fallbackSource})`);
  console.log(`# Owners:            ${args.owners.join(', ')}`);
  console.log(`# Threshold:         ${args.threshold}`);
  console.log('');
  console.log('--factory', factoryAddress);
  console.log('--init-code-hash', '0x' + initCodeHash);
  console.log('--initializer-hash', '0x' + initializerHash);
  console.log('');
  console.log('# Example safe_vanity command:');
  console.log(
    `# safe_vanity -p dead --factory ${factoryAddress} --init-code-hash 0x${initCodeHash} --initializer-hash 0x${initializerHash}`
  );
  console.log('');
  console.log('# After mining, deploy with:');
  console.log('# node deploy.js --chain-id', chainId, '--owners', args.owners.join(','),
    '--threshold', args.threshold, '--salt-nonce <DECIMAL_SALT_FROM_MINER>',
    '--rpc-url', args.rpcUrl, '--private-key <DEPLOYER_KEY>');
}

main().catch((err) => {
  console.error('Error:', err.message || err);
  process.exit(1);
});
