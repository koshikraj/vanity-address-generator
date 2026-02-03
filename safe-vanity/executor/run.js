#!/usr/bin/env node
/**
 * Single-flow Safe vanity: config → fetch → mine → verify → [deploy with confirmation].
 *
 * Required: --owners, --threshold, --pattern.
 * Add --deploy to deploy after mining; you will be prompted for confirmation and need --private-key or SAFE_DEPLOYER_PRIVATE_KEY.
 * Standalone deploy.js remains for deploying with an existing nonce (e.g. from API).
 *
 * Usage:
 *   node run.js --owners 0x...,0x... --threshold 1 --pattern dead
 *   node run.js --owners 0x... --threshold 1 -p dead --deploy --private-key <hex>
 *   node run.js --owners 0x... -p dead -s beef --deploy   # private key from .env or SAFE_DEPLOYER_PRIVATE_KEY
 *
 * Pattern options: -s/--suffix, -t/--pattern-type (prefix|suffix|contains), -c/--case-sensitive.
 */

import 'dotenv/config';
import { spawn } from 'child_process';
import { existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import * as readline from 'readline';
import { loadConfig, fetchSafeConfig, predictSafeAddress, computeCreate2Address, toChecksumAddress, saltNonceDecimalToBytes } from './lib/safe-config.js';
import { deploySafe } from './lib/deploy.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const MINER_ROOT = join(__dirname, '..', 'miner');

/** Resolve miner binary: local target/release or target/debug, or use cargo run. Returns { cmd, args, cwd } for spawn. */
function resolveMiner(minerPath, minerArgs) {
  const raw = minerPath || 'safe_vanity';
  if (raw.includes('/') || raw.includes('\\')) {
    return { cmd: raw, args: minerArgs, cwd: undefined };
  }
  const release = join(MINER_ROOT, 'target', 'release', 'safe_vanity');
  const debug = join(MINER_ROOT, 'target', 'debug', 'safe_vanity');
  if (existsSync(release)) return { cmd: release, args: minerArgs, cwd: undefined };
  if (existsSync(debug)) return { cmd: debug, args: minerArgs, cwd: undefined };
  if (raw === 'safe_vanity') {
    return { cmd: 'cargo', args: ['run', '-p', 'safe_vanity', '--', ...minerArgs], cwd: MINER_ROOT };
  }
  return { cmd: raw, args: minerArgs, cwd: undefined };
}

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    owners: null,
    threshold: null,
    pattern: null,
    suffix: null,
    patternType: null,
    caseSensitive: null,
    config: null,
    chainId: null,
    rpcUrl: null,
    useL2: null,
    safeVersion: null,
    fallbackHandler: null,
    minerPath: null,
    workers: null,
    count: null,
    reportInterval: null,
    deploy: false,
    privateKey: null,
  };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--owners' && args[i + 1]) out.owners = args[++i];
    else if (args[i] === '--threshold' && args[i + 1]) out.threshold = args[++i];
    else if ((args[i] === '--pattern' || args[i] === '-p') && args[i + 1]) out.pattern = args[++i];
    else if ((args[i] === '--suffix' || args[i] === '-s') && args[i + 1]) out.suffix = args[++i];
    else if ((args[i] === '--pattern-type' || args[i] === '-t') && args[i + 1]) out.patternType = args[++i];
    else if (args[i] === '--case-sensitive' || args[i] === '-c') out.caseSensitive = true;
    else if (args[i] === '--config' && args[i + 1]) out.config = args[++i];
    else if (args[i] === '--chain-id' && args[i + 1]) out.chainId = args[++i];
    else if (args[i] === '--rpc-url' && args[i + 1]) out.rpcUrl = args[++i];
    else if (args[i] === '--l2') out.useL2 = true;
    else if (args[i] === '--safe-version' && args[i + 1]) out.safeVersion = args[++i];
    else if (args[i] === '--fallback-handler' && args[i + 1]) out.fallbackHandler = args[++i];
    else if (args[i] === '--miner-path' && args[i + 1]) out.minerPath = args[++i];
    else if ((args[i] === '--workers' || args[i] === '-w') && args[i + 1]) out.workers = args[++i];
    else if ((args[i] === '--count' || args[i] === '-n') && args[i + 1]) out.count = args[++i];
    else if ((args[i] === '--report-interval' || args[i] === '-r') && args[i + 1]) out.reportInterval = args[++i];
    else if (args[i] === '--deploy') out.deploy = true;
    else if (args[i] === '--private-key' && args[i + 1]) out.privateKey = args[++i];
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
  if (cli.minerPath != null) c.minerPath = cli.minerPath;
  if (cli.workers != null) c.workers = cli.workers;
  if (cli.count != null) c.count = parseInt(cli.count, 10);
  if (cli.reportInterval != null) c.reportInterval = parseInt(cli.reportInterval, 10);
  if (cli.suffix != null) c.suffix = cli.suffix;
  if (cli.patternType != null) c.patternType = cli.patternType;
  if (cli.caseSensitive != null) c.caseSensitive = cli.caseSensitive;
  if (cli.deploy != null) c.deploy = cli.deploy;
  if (cli.privateKey != null) c.privateKey = cli.privateKey;
  return c;
}

async function main() {
  const cli = parseArgs();

  if (!cli.owners || !cli.threshold || !cli.pattern) {
    console.error('Usage: node run.js --owners <0x...,0x...> --threshold <n> --pattern <hex>');
    console.error('');
    console.error('Required:');
    console.error('  --owners <addrs>    Comma-separated owner addresses');
    console.error('  --threshold <n>     Number of required signatures');
    console.error('  --pattern <hex>     Vanity pattern (e.g. dead, cafe)');
    console.error('');
    console.error('Optional (override config):');
    console.error('  -s, --suffix <hex>       Suffix pattern (pattern becomes prefix)');
    console.error('  -t, --pattern-type <t>   prefix | suffix | contains (default: prefix)');
    console.error('  -c, --case-sensitive    Case-sensitive matching');
    console.error('  --config <path>     Config file path');
    console.error('  --chain-id <id>     Chain ID');
    console.error('  --rpc-url <url>     RPC URL');
    console.error('  --l2                Use SafeL2 singleton');
    console.error('  --safe-version <v>  Safe version (e.g. 1.4.1)');
    console.error('  --miner-path <bin>  Path to safe_vanity binary');
    console.error('  -w, --workers <n>   Worker threads');
    console.error('  -n, --count <n>      Stop after N matches (default 1)');
    console.error('  --deploy             Deploy Safe after mining (confirmation prompt)');
    console.error('  --private-key <hex>  Deployer key (or set SAFE_DEPLOYER_PRIVATE_KEY)');
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

  console.log('Safe Vanity — single flow');
  console.log('========================');
  console.log('Config:    ', config.chainId, config.useL2 ? '(L2)' : '(L1)', 'Safe', config.safeVersion);
  console.log('Owners:    ', owners.join(', '));
  console.log('Threshold: ', threshold);
  const patternDisplay = config.suffix
    ? `prefix "${cli.pattern}" + suffix "${config.suffix}"`
    : `${cli.pattern} (${config.patternType || 'prefix'})`;
  console.log('Pattern:   ', patternDisplay);
  if (config.caseSensitive) console.log('Case:       sensitive');
  console.log('');

  console.log('Fetching Safe config (factory, init_code_hash, initializer_hash)...');
  let safeConfig;
  try {
    safeConfig = await fetchSafeConfig(owners, threshold, config);
  } catch (err) {
    console.error('Error:', err.message);
    process.exit(1);
  }

  console.log('Factory:   ', safeConfig.factory);
  console.log('Init hash: ', safeConfig.initCodeHash.slice(0, 18) + '...');
  console.log('Initl hash:', safeConfig.initializerHash.slice(0, 18) + '...');
  console.log('');

  const minerArgs = [
    '-p', cli.pattern,
    '--factory', safeConfig.factory,
    '--init-code-hash', safeConfig.initCodeHash,
    '--initializer-hash', safeConfig.initializerHash,
    '-n', String(config.count ?? 1),
    '-r', String(config.reportInterval ?? 5),
  ];
  if (config.suffix != null) minerArgs.push('-s', config.suffix);
  if (config.patternType && config.patternType !== 'prefix') minerArgs.push('-t', config.patternType);
  if (config.caseSensitive) minerArgs.push('-c');
  if (config.workers != null) minerArgs.push('-w', String(config.workers));

  const { cmd, args, cwd } = resolveMiner(config.minerPath, minerArgs);
  console.log('Starting miner:', cwd ? `cargo run (${cwd})` : cmd, args.join(' '));
  console.log('');

  const proc = spawn(cmd, args, {
    stdio: ['inherit', 'pipe', 'inherit'],
    shell: false,
    cwd: cwd || undefined,
  });

  let lastAddress = null;
  let lastSaltDec = null;

  proc.stdout.setEncoding('utf8');
  proc.stdout.on('data', (chunk) => {
    process.stdout.write(chunk);
    const lines = chunk.split('\n');
    for (const line of lines) {
      const addrMatch = /^Address:\s+(0x[a-fA-F0-9]{40})/.exec(line);
      if (addrMatch) lastAddress = addrMatch[1];
      const saltMatch = /^Salt \(dec\):\s+(\d+)/.exec(line);
      if (saltMatch) lastSaltDec = saltMatch[1];
    }
  });

  await new Promise((resolve, reject) => {
    proc.on('error', (err) => {
      console.error('');
      console.error('Failed to run miner:', err.message);
      if ((config.minerPath || 'safe_vanity') === 'safe_vanity') {
        console.error('Build the miner: cd', MINER_ROOT, '&& cargo build --release');
        console.error('Or set minerPath in safe-vanity.config.json to the full path of safe_vanity.');
      }
      reject(err);
    });
    proc.on('exit', (code, signal) => {
      if (code !== 0 && code != null) reject(new Error(`Miner exited with code ${code}`));
      else resolve();
    });
  });

  if (lastAddress != null && lastSaltDec != null) {
    const prediction = await predictSafeAddress(owners, threshold, lastSaltDec, config);
    const match = prediction.address.toLowerCase() === lastAddress.toLowerCase();

    console.log('');
    console.log('--- Verification ---');
    console.log('Miner address: ', lastAddress);
    console.log('Predicted:     ', prediction.address, match ? '(match)' : '(MISMATCH)');
    console.log('Salt nonce:    ', prediction.saltNonce, '(hex:', prediction.saltNonceHex + ')');
    if (!match) {
      console.error('Formula verification failed.');
      process.exit(1);
    }

    if (config.deploy) {
      const privateKey = cli.privateKey || process.env.SAFE_DEPLOYER_PRIVATE_KEY;
      console.log('');
      console.log('--- Deploy confirmation ---');
      console.log('Chain ID:      ', prediction.safeConfig.chainId);
      console.log('Safe address:  ', prediction.address);
      console.log('Salt nonce:    ', prediction.saltNonce);
      console.log('Factory:       ', prediction.safeConfig.factory);
      console.log('Singleton:     ', prediction.safeConfig.singletonAddress);
      console.log('');
      const confirmed = await confirm('Deploy this Safe? (y/N) ');
      if (!confirmed) {
        console.log('Deploy cancelled.');
        console.log('');
        console.log('To deploy later: node deploy.js --owners', cli.owners, '--threshold', threshold, '--salt-nonce', prediction.saltNonce, '--deploy --private-key <KEY>');
        return;
      }
      console.log('Deploying...');
      try {
        const result = await deploySafe(owners, threshold, prediction.saltNonce, privateKey, config);
        console.log('Transaction hash:', result.txHash);
        console.log('Confirmed in block:', result.receipt.blockNumber.toString());
        console.log('Safe deployed at:', result.address);
      } catch (err) {
        console.error('Deploy failed:', err.message);
        process.exit(1);
      }
    } else {
      console.log('');
      console.log('--- Deploy later (same config) ---');
      console.log(
        'node deploy.js --chain-id', prediction.safeConfig.chainId,
        '--owners', owners.join(','),
        '--threshold', threshold,
        '--salt-nonce', prediction.saltNonce,
        '--rpc-url', prediction.safeConfig.rpcUrl,
        config.useL2 ? '--l2' : '',
        '--deploy --private-key <KEY>'
      );
      console.log('');
    }
  }
}

function confirm(question) {
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((resolve) => {
    rl.question(question, (answer) => {
      rl.close();
      const a = (answer || '').trim().toLowerCase();
      resolve(a === 'y' || a === 'yes');
    });
  });
}

main().catch((err) => {
  console.error('Error:', err.message || err);
  process.exit(1);
});
