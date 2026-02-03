/**
 * Deploy Safe with given owners, threshold, saltNonce and private key.
 * Reusable for run.js (--deploy) and standalone deploy.js.
 */

import { createPublicClient, createWalletClient, http, encodeFunctionData } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import {
  fetchSafeConfig,
  ensure0x,
  ZERO,
  SAFE_SETUP_ABI,
  computeCreate2Address,
  toChecksumAddress,
  saltNonceDecimalToBytes,
} from './safe-config.js';

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

/**
 * Deploy a Safe with the given salt nonce. Uses config for chain/RPC/Safe version.
 *
 * @param {string[]} owners - Owner addresses
 * @param {number} threshold - Required signatures
 * @param {string|bigint} saltNonce - Salt nonce (decimal string or bigint)
 * @param {string} privateKey - Deployer private key (hex with or without 0x)
 * @param {object} config - Chain config (chainId, rpcUrl, useL2, safeVersion, etc.)
 * @returns {Promise<{ txHash: string, receipt: object, address: string }>}
 */
export async function deploySafe(owners, threshold, saltNonce, privateKey, config) {
  const safeConfig = await fetchSafeConfig(owners, threshold, config);
  const saltNonceBigInt = typeof saltNonce === 'bigint' ? saltNonce : BigInt(String(saltNonce));

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

  const publicClient = createPublicClient({ transport: http(safeConfig.rpcUrl) });
  const account = privateKeyToAccount(ensure0x(privateKey));
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

  const receipt = await publicClient.waitForTransactionReceipt({ hash: txHash });

  const saltNonceBytes = saltNonceDecimalToBytes(saltNonceBigInt.toString());
  const addressBytes = computeCreate2Address(
    safeConfig.factory,
    safeConfig.initCodeHash,
    safeConfig.initializerHash,
    saltNonceBytes
  );
  const address = toChecksumAddress(addressBytes);

  return { txHash, receipt, address };
}
