/**
 * @redflag/sdk — RedflagClient
 * Base HTTP client for the RedFlag Network RPC.
 *
 * Usage:
 *   import { RedflagClient } from '@redflag/sdk'
 *   const rf = new RedflagClient()
 */

export const RF_MAINNET = 'https://redflagweb3-node1.onrender.com';
export const RF_NODE2   = 'https://redflagweb3-node2.onrender.com';
export const RF_NODE3   = 'https://redflagweb3-node3.onrender.com';

export const CHAIN_ID  = 2100;
export const DECIMALS  = 6;
export const SYMBOL    = 'RF';

export class RedflagClient {
  /**
   * @param {string} [nodeUrl] - RPC URL (default: mainnet node1)
   * @param {{ timeout?: number }} [opts]
   */
  constructor(nodeUrl = RF_MAINNET, opts = {}) {
    this.nodeUrl = nodeUrl.replace(/\/$/, '');
    this.timeout = opts.timeout ?? 30_000;
  }

  /** Low-level fetch wrapper */
  async _fetch(path, options = {}) {
    const url = `${this.nodeUrl}${path}`;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);
    try {
      const res = await fetch(url, {
        headers: { 'Content-Type': 'application/json', ...options.headers },
        signal: controller.signal,
        ...options,
      });
      if (!res.ok) {
        const text = await res.text().catch(() => '');
        throw new Error(`RF RPC ${res.status}: ${text}`);
      }
      return res.json();
    } finally {
      clearTimeout(timer);
    }
  }

  _get(path) { return this._fetch(path); }
  _post(path, body) {
    return this._fetch(path, { method: 'POST', body: JSON.stringify(body) });
  }

  // ── Chain Info ────────────────────────────────────────────────────────────

  /** Returns chain-level stats: block height, TPS, validators, supply */
  async getChainInfo() {
    return this._get('/api/v1/summary');
  }

  /** Returns ticker: RF price, volume, market cap */
  async getTicker() {
    return this._get('/api/v1/ticker');
  }

  /** Returns available assets on the network */
  async getAssets() {
    return this._get('/api/v1/assets');
  }

  // ── Accounts ─────────────────────────────────────────────────────────────

  /**
   * Get RF balance for an address.
   * Returns raw units (divide by 10^6 for human-readable).
   * @param {string} address
   */
  async getBalance(address) {
    const data = await this._get(`/balance/${address}`);
    return {
      raw: data.balance ?? data,
      rf: (Number(data.balance ?? data) / 1_000_000).toFixed(6),
      address,
    };
  }

  // ── Transactions ──────────────────────────────────────────────────────────

  /**
   * Submit a signed transaction to the network.
   * @param {{ from, to, amount, nonce, signature, public_key }} tx
   */
  async sendTransaction(tx) {
    return this._post('/tx', tx);
  }

  /**
   * Submit an encrypted (E2E) transaction.
   * @param {{ ciphertext, sender_pub, recipient_pub, nonce }} encTx
   */
  async sendEncryptedTransaction(encTx) {
    return this._post('/tx/encrypted', encTx);
  }
}
