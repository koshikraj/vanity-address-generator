#!/usr/bin/env node
/**
 * Single-flow Safe vanity: config → fetch Safe params → mine → verify.
 *
 * Only required args: --owners, --threshold, --pattern.
 * Everything else (chain, RPC, L2, Safe version, etc.) comes from safe-vanity.config.json.
 *
 * Usage:
 *   node run.js --owners 0x...,0x... --threshold 1 --pattern dead
 *   node run.js --owners 0x... --threshold 1 -p dead -s beef     # prefix dead + suffix beef
 *   node run.js --owners 0x... --threshold 1 -p cafe -t suffix   # suffix only
 *   node run.js --owners 0x... --threshold 1 -p cafe -c          # case-sensitive
 *
 * Pattern options (same as Rust safe_vanity): -s/--suffix, -t/--pattern-type (prefix|suffix|contains), -c/--case-sensitive.
 * Config file: chainId, rpcUrl, useL2, safeVersion, fallbackHandler, minerPath, workers, count, reportInterval, suffix, patternType, caseSensitive.
 */

import { spawn } from 'child_process';
import { existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { loadConfig, fetchSafeConfig, computeCreate2Address, toChecksumAddress, saltNonceDecimalToBytes } from './lib/safe-config.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SAFE_VANITY_ROOT = join(__dirname, '..');

/** Resolve miner binary: local target/release or target/debug, or use cargo run. Returns { cmd, args, cwd } for spawn. */
function resolveMiner(minerPath, minerArgs) {
  const raw = minerPath || 'safe_vanity';
  if (raw.includes('/') || raw.includes('\\')) {
    return { cmd: raw, args: minerArgs, cwd: undefined };
  }
  const release = join(SAFE_VANITY_ROOT, 'target', 'release', 'safe_vanity');
  const debug = join(SAFE_VANITY_ROOT, 'target', 'debug', 'safe_vanity');
  if (existsSync(release)) return { cmd: release, args: minerArgs, cwd: undefined };
  if (existsSync(debug)) return { cmd: debug, args: minerArgs, cwd: undefined };
  if (raw === 'safe_vanity') {
    return { cmd: 'cargo', args: ['run', '-p', 'safe_vanity', '--', ...minerArgs], cwd: SAFE_VANITY_ROOT };
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
  return c;
}

function ensure0x(hex) {
  return hex.startsWith('0x') ? hex : '0x' + hex;
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
        console.error('Build the miner: cd', SAFE_VANITY_ROOT, '&& cargo build --release');
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
    const saltBytes = saltNonceDecimalToBytes(lastSaltDec);
    const computed = computeCreate2Address(
      safeConfig.factory,
      safeConfig.initCodeHash,
      safeConfig.initializerHash,
      saltBytes
    );
    const computedChecksum = toChecksumAddress(computed);
    const match = computedChecksum.toLowerCase() === lastAddress.toLowerCase();

    console.log('');
    console.log('--- Verification ---');
    console.log('Miner address: ', lastAddress);
    console.log('Formula check: ', computedChecksum, match ? '(match)' : '(MISMATCH)');
    if (!match) {
      console.error('Formula verification failed.');
      process.exit(1);
    }
    console.log('');
    console.log('--- Deploy (same config) ---');
    console.log(
      'node deploy.js --chain-id', safeConfig.chainId,
      '--owners', owners.join(','),
      '--threshold', threshold,
      '--salt-nonce', lastSaltDec,
      '--rpc-url', safeConfig.rpcUrl,
      config.useL2 ? '--l2' : '',
      '--deploy --private-key <KEY>'
    );
    console.log('');
  }
}

main().catch((err) => {
  console.error('Error:', err.message || err);
  process.exit(1);
});
