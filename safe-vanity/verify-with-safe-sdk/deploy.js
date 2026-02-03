#!/usr/bin/env node
/**
 * Deploy a Safe using an existing salt nonce (e.g. from miner output or REST API).
 * Uses config file for defaults; only owners, threshold, salt-nonce required.
 * Use this when you have a nonce and want to deploy without running the miner.
 *
 * Usage:
 *   node deploy.js --owners 0x...,0x... --threshold 1 --salt-nonce 12345
 *   node deploy.js --owners 0x... --threshold 1 --salt-nonce 12345 --deploy --private-key <hex>
 *   SAFE_DEPLOYER_PRIVATE_KEY in .env or env for --deploy without --private-key
 */

import 'dotenv/config';
import { loadConfig, predictSafeAddress } from './lib/safe-config.js';
import { deploySafe } from './lib/deploy.js';

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

  if (cli.deploy && !cli.privateKey && !process.env.SAFE_DEPLOYER_PRIVATE_KEY) {
    console.error('Error: --deploy requires --private-key or SAFE_DEPLOYER_PRIVATE_KEY');
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

  let prediction;
  try {
    prediction = await predictSafeAddress(owners, threshold, cli.saltNonce, config);
  } catch (err) {
    console.error('Error:', err.message);
    process.exit(1);
  }

  console.log('Safe Deployment');
  console.log('===============');
  console.log('Chain ID:          ', prediction.safeConfig.chainId);
  console.log('Owners:            ', owners.join(', '));
  console.log('Threshold:         ', threshold);
  console.log('Salt nonce (dec):  ', prediction.saltNonce);
  console.log('Factory:           ', prediction.safeConfig.factory);
  console.log('Singleton:         ', prediction.safeConfig.singletonAddress);
  console.log('Fallback handler:  ', prediction.safeConfig.fallbackHandler);
  console.log('');
  console.log('Predicted address: ', prediction.address);

  if (!cli.deploy) {
    console.log('');
    console.log('Dry run. Deploy with: node deploy.js --owners', cli.owners, '--threshold', threshold, '--salt-nonce', cli.saltNonce, '--deploy --private-key <KEY>');
    return;
  }

  console.log('');
  console.log('Deploying Safe...');

  const privateKey = cli.privateKey || process.env.SAFE_DEPLOYER_PRIVATE_KEY;
  try {
    const result = await deploySafe(owners, threshold, cli.saltNonce, privateKey, config);
    console.log('Transaction hash:  ', result.txHash);
    console.log('Confirmed in block:', result.receipt.blockNumber.toString());
    console.log('Status:            ', result.receipt.status === 'success' ? 'SUCCESS' : 'FAILED');
    if (result.receipt.status === 'success') {
      console.log('');
      console.log('Safe deployed at:  ', result.address);
    } else {
      process.exit(1);
    }
  } catch (err) {
    console.error('Error:', err.message);
    process.exit(1);
  }
}

main().catch((err) => {
  console.error('Error:', err.message || err);
  process.exit(1);
});
