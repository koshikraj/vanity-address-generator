#!/usr/bin/env node
/**
 * Fetch factory, init_code_hash, and initializer_hash from Safe config for use with safe_vanity.
 * Uses config file (safe-vanity.config.json) for defaults; only owners and threshold required.
 *
 * Usage:
 *   node fetch-config.js --owners 0x...,0x... --threshold 1
 *   node fetch-config.js --owners 0x... --threshold 1 --chain-id 1 --rpc-url <url>
 */

import { loadConfig, fetchSafeConfig } from './lib/safe-config.js';

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

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    owners: null,
    threshold: null,
    config: null,
    chainId: null,
    rpcUrl: null,
    fallbackHandler: null,
    useL2: false,
    safeVersion: null,
  };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--owners' && args[i + 1]) out.owners = args[++i];
    else if (args[i] === '--threshold' && args[i + 1]) out.threshold = args[++i];
    else if (args[i] === '--config' && args[i + 1]) out.config = args[++i];
    else if (args[i] === '--chain-id' && args[i + 1]) out.chainId = args[++i];
    else if (args[i] === '--rpc-url' && args[i + 1]) out.rpcUrl = args[++i];
    else if (args[i] === '--fallback-handler' && args[i + 1]) out.fallbackHandler = args[++i];
    else if (args[i] === '--l2') out.useL2 = true;
    else if (args[i] === '--safe-version' && args[i + 1]) out.safeVersion = args[++i];
  }
  return out;
}

function mergeConfig(fileConfig, cli) {
  const c = { ...fileConfig };
  if (cli.chainId != null) c.chainId = cli.chainId;
  if (cli.rpcUrl != null) c.rpcUrl = cli.rpcUrl;
  if (cli.useL2) c.useL2 = true;
  if (cli.safeVersion != null) c.safeVersion = cli.safeVersion;
  if (cli.fallbackHandler != null) c.fallbackHandler = cli.fallbackHandler;
  return c;
}

async function main() {
  const cli = parseArgs();

  if (!cli.owners || !cli.threshold) {
    console.error('Usage: node fetch-config.js --owners <0x...,0x...> --threshold <n>');
    console.error('');
    console.error('Required: --owners, --threshold. Rest from safe-vanity.config.json or overrides below.');
    console.error('Optional: --config, --chain-id, --rpc-url, --l2, --safe-version, --fallback-handler');
    process.exit(1);
  }

  const owners = cli.owners.split(',').map((a) => a.trim());
  const threshold = parseInt(cli.threshold, 10);
  const fileConfig = loadConfig(cli.config);
  const config = mergeConfig(fileConfig, cli);

  if (!config.rpcUrl) {
    console.error('Error: rpcUrl required. Set in safe-vanity.config.json or pass --rpc-url <url>');
    process.exit(1);
  }

  const chainName = CHAIN_NAMES[config.chainId] || `Chain ${config.chainId}`;

  let safeConfig;
  try {
    safeConfig = await fetchSafeConfig(owners, threshold, config);
  } catch (err) {
    console.error('Error:', err.message);
    process.exit(1);
  }

  console.log(`# Safe vanity config for ${chainName} (chain ${safeConfig.chainId})`);
  console.log(`# Safe version:      ${safeConfig.safeVersion}`);
  console.log(`# Factory:           ${safeConfig.factory}`);
  console.log(`# Singleton:         ${safeConfig.singletonAddress} ${config.useL2 ? '(L2)' : ''}`);
  console.log(`# Fallback handler:  ${safeConfig.fallbackHandler}`);
  console.log(`# Owners:            ${owners.join(', ')}`);
  console.log(`# Threshold:         ${safeConfig.threshold}`);
  console.log('');
  console.log('--factory', safeConfig.factory);
  console.log('--init-code-hash', safeConfig.initCodeHash);
  console.log('--initializer-hash', safeConfig.initializerHash);
  console.log('');
  console.log('# Mine with: node run.js --owners', cli.owners, '--threshold', threshold, '--pattern <hex>');
  console.log('# Or: safe_vanity -p dead --factory', safeConfig.factory, '--init-code-hash', safeConfig.initCodeHash, '--initializer-hash', safeConfig.initializerHash);
  console.log('');
  console.log('# Deploy: node deploy.js --owners', cli.owners, '--threshold', threshold, '--salt-nonce <DECIMAL> --rpc-url', config.rpcUrl, '--deploy --private-key <KEY>');
}

main().catch((err) => {
  console.error('Error:', err.message || err);
  process.exit(1);
});
