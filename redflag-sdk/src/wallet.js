/**
 * @redflag/sdk — WalletHelper
 * Utility helpers for working with RF wallets.
 *
 * NOTE: The RedFlag Network uses ML-DSA-65 (post-quantum) signatures.
 * Key generation and signing is done server-side by the RF node.
 * This module provides browser-compatible helpers for common operations.
 */

import { DECIMALS, SYMBOL } from './client.js';

// ── Unit conversion ───────────────────────────────────────────────────────────

/**
 * Convert raw RF units to human-readable string.
 * @param {number|bigint} raw - Raw units (6 decimal places)
 * @returns {string} e.g. "1.500000"
 */
export function fromRawRF(raw) {
  return (Number(raw) / 10 ** DECIMALS).toFixed(DECIMALS);
}

/**
 * Convert human-readable RF to raw units.
 * @param {number|string} amount - e.g. 1.5
 * @returns {number}
 */
export function toRawRF(amount) {
  return Math.round(Number(amount) * 10 ** DECIMALS);
}

/**
 * Format RF balance for display.
 * @param {number|bigint} raw
 * @returns {string} e.g. "1.500000 RF"
 */
export function formatRF(raw) {
  return `${fromRawRF(raw)} ${SYMBOL}`;
}

// ── Address validation ────────────────────────────────────────────────────────

/**
 * Validate a RedFlag address (64-char hex public key).
 * @param {string} address
 */
export function isValidAddress(address) {
  return typeof address === 'string' && /^[0-9a-fA-F]{64}$/.test(address);
}

// ── Chain config for wallets (viem/wagmi/ethers) ──────────────────────────────

/**
 * RedFlag Network chain config compatible with viem/wagmi.
 * Add to your Wagmi config to allow users to connect to chain 2100.
 *
 * @example
 * import { createConfig } from 'wagmi'
 * import { RF_CHAIN } from '@redflag/sdk/wallet'
 * const config = createConfig({ chains: [RF_CHAIN, polygon] })
 */
export const RF_CHAIN = {
  id: 2100,
  name: 'RedFlag Network',
  nativeCurrency: { name: 'RedFlag', symbol: 'RF', decimals: 6 },
  rpcUrls: {
    default: { http: ['https://redflagweb3-node1.onrender.com'] },
    public:  { http: ['https://redflagweb3-node1.onrender.com'] },
  },
  blockExplorers: {
    default: {
      name: 'RedFlag Explorer',
      url: 'https://redflagweb3-node1.onrender.com',
    },
  },
  testnet: false,
};

/**
 * MetaMask-compatible network params for adding RF chain.
 * Pass to `wallet_addEthereumChain`.
 *
 * @example
 * await window.ethereum.request({
 *   method: 'wallet_addEthereumChain',
 *   params: [ADD_RF_NETWORK_PARAMS]
 * })
 */
export const ADD_RF_NETWORK_PARAMS = {
  chainId: '0x834',           // 2100 in hex
  chainName: 'RedFlag Network',
  nativeCurrency: { name: 'RedFlag', symbol: 'RF', decimals: 6 },
  rpcUrls: ['https://redflagweb3-node1.onrender.com'],
  blockExplorerUrls: ['https://redflagweb3-node1.onrender.com'],
};
