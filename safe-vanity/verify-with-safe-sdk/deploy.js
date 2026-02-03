#!/usr/bin/env node
/**
 * Deploy a Safe using a mined saltNonce from safe_vanity.
 * Uses config file (safe-vanity.config.json) for defaults; only owners, threshold, salt-nonce required.
 *
 * Usage:
 *   node deploy.js --owners 0x...,0x... --threshold 1 --salt-nonce 12345
 *   node deploy.js --owners 0x... --threshold 1 --salt-nonce 12345 --deploy --private-key <hex>
 */

import { createPublicClient, createWalletClient, http, encodeFunctionData } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import {
  loadConfig,
  fetchSafeConfig,
  computeCreate2Address,
  toChecksumAddress,
  saltNonceDecimalToBytes,
  ensure0x,
  ZERO,
} from './lib/safe-config.js';

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

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    owners: null,
    threshold: null,
    saltNonce: null,
    config: null,
    rpcUrl: null,
    privateKey: null,
    deploy: false,
    chainId: null,
    useL2: null,
    fallbackHandler: null,
    safeVersion: null,
  };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--owners' && args[i + 1]) out.owners = args[++i];
    else if (args[i] === '--threshold' && args[i + 1]) out.threshold = args[++i];
    else if (args[i] === '--salt-nonce' && args[i + 1]) out.saltNonce = args[++i];
    else if (args[i] === '--config' && args[i + 1]) out.config = args[++i];
    else if (args[i] === '--rpc-url' && args[i + 1]) out.rpcUrl = args[++i];
    else if (args[i] === '--private-key' && args[i + 1]) out.privateKey = args[++i];
    else if (args[i] === '--deploy') out.deploy = true;
    else if (args[i] === '--chain-id' && args[i + 1]) out.chainId = args[++i];
    else if (args[i] === '--l2') out.useL2 = true;
    else if (args[i] === '--fallback-handler' && args[i + 1]) out.fallbackHandler = args[++i];
    else if (args[i] === '--safe-version' && args[i + 1]) out.safeVersion = args[++i];
  }
  return out;
}

function mergeConfig(fileConfig, cli) {
  const c = { ...fileConfig };
  if (cli.chainId != null) c.chainId = cli.chainId;
  if (cli.rpcUrl != null) c.rpcUrl = cli.rpcUrl;
  if (cli.useL2 != null) c.useL2 = cli.useL2;
  if (cli.safeVersion != null) c.safeVersion = cli.safeVersion;
  if (cli.fallbackHandler != null) c.fallbackHandler = cli.fallbackHandler;
  return c;
}

async function main() {
  const cli = parseArgs();

  if (!cli.owners || !cli.threshold || !cli.saltNonce) {
    console.error('Usage: node deploy.js --owners <0x...,0x...> --threshold <n> --salt-nonce <decimal>');
    console.error('');
    console.error('Required: --owners, --threshold, --salt-nonce. Rest from safe-vanity.config.json.');
    console.error('Optional: --config, --chain-id, --rpc-url, --l2, --safe-version, --deploy, --private-key');
    process.exit(1);
  }

  if (cli.deploy && !cli.privateKey) {
    console.error('Error: --private-key is required when using --deploy');
    process.exit(1);
  }

  const owners = cli.owners.split(',').map((a) => a.trim());
  const threshold = parseInt(cli.threshold, 10);
  const fileConfig = loadConfig(cli.config);
  const config = mergeConfig(fileConfig, cli);

  if (!config.rpcUrl) {
    console.error('Error: rpcUrl required. Set in safe-vanity.config.json or pass --rpc-url');
    process.exit(1);
  }

  let safeConfig;
  try {
    safeConfig = await fetchSafeConfig(owners, threshold, config);
  } catch (err) {
    console.error('Error:', err.message);
    process.exit(1);
  }

  const initializerCalldata = encodeFunctionData({
    abi: SAFE_SETUP_ABI,
    functionName: 'setup',
    args: [
      owners.map(ensure0x),
      BigInt(threshold),
      ZERO,
      '0x',
      safeConfig.fallbackHandler,
      ZERO,
      BigInt(0),
      ZERO,
    ],
  });

  const saltNonceBytes = saltNonceDecimalToBytes(cli.saltNonce);
  const saltNonceBigInt = BigInt(cli.saltNonce);
  const formulaAddress = computeCreate2Address(
    safeConfig.factory,
    safeConfig.initCodeHash,
    safeConfig.initializerHash,
    saltNonceBytes
  );
  const formulaChecksum = toChecksumAddress(formulaAddress);

  console.log('Safe Deployment');
  console.log('===============');
  console.log('Chain ID:          ', safeConfig.chainId);
  console.log('Owners:            ', owners.join(', '));
  console.log('Threshold:         ', threshold);
  console.log('Salt nonce (dec):  ', cli.saltNonce);
  console.log('Factory:           ', safeConfig.factory);
  console.log('Singleton:         ', safeConfig.singletonAddress);
  console.log('Fallback handler:  ', safeConfig.fallbackHandler);
  console.log('');
  console.log('Predicted address: ', formulaChecksum);

  if (!cli.deploy) {
    console.log('');
    console.log('Dry run. Deploy with: node deploy.js --owners', cli.owners, '--threshold', threshold, '--salt-nonce', cli.saltNonce, '--deploy --private-key <KEY>');
    return;
  }

  console.log('');
  console.log('Deploying Safe...');

  const publicClient = createPublicClient({ transport: http(safeConfig.rpcUrl) });
  const account = privateKeyToAccount(ensure0x(cli.privateKey));
  const walletClient = createWalletClient({
    account,
    transport: http(safeConfig.rpcUrl),
  });

  const txHash = await walletClient.writeContract({
    address: safeConfig.factory,
    abi: FACTORY_ABI,
    functionName: 'createProxyWithNonce',
    args: [safeConfig.singletonAddress, initializerCalldata, saltNonceBigInt],
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

  const code = await publicClient.getCode({ address: formulaChecksum });
  if (code && code !== '0x') {
    console.log('');
    console.log('Safe deployed at:  ', formulaChecksum);
    console.log('Verified: contract code exists at predicted address.');
  } else {
    console.error('');
    console.error('WARNING: No code found at predicted address', formulaChecksum);
    console.error('Check the transaction logs for the ProxyCreation event.');
  }
}

main().catch((err) => {
  console.error('Error:', err.message || err);
  process.exit(1);
});
