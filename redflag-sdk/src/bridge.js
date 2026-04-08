/**
 * @redflag/sdk — BridgeClient
 * Cross-chain bridge: EVM (ETH/BSC/MATIC) ↔ RedFlag Network.
 *
 * Bridge URL: https://redflagweb3-bridge.onrender.com
 */

export const BRIDGE_URL = 'https://redflagweb3-bridge.onrender.com';

export const SUPPORTED_CHAINS = {
  ethereum: { chainId: 1,   symbol: 'wETH',  name: 'Ethereum Mainnet' },
  bsc:      { chainId: 56,  symbol: 'wBNB',  name: 'BNB Smart Chain'  },
  polygon:  { chainId: 137, symbol: 'wMATIC', name: 'Polygon'          },
};

export class BridgeClient {
  constructor(bridgeUrl = BRIDGE_URL) {
    this.bridgeUrl = bridgeUrl.replace(/\/$/, '');
  }

  async _fetch(path, options = {}) {
    const res = await fetch(`${this.bridgeUrl}${path}`, {
      headers: { 'Content-Type': 'application/json', ...options.headers },
      ...options,
    });
    if (!res.ok) throw new Error(`Bridge ${res.status}: ${await res.text()}`);
    return res.json();
  }

  /** Bridge status: connected chains, pending events, completed count */
  async getStatus() {
    return this._fetch('/bridge/status');
  }

  /**
   * Get bridge history for an EVM address.
   * @param {string} evmAddress - 0x... address
   */
  async getHistory(evmAddress) {
    return this._fetch(`/bridge/history/${evmAddress}`);
  }

  /**
   * Estimate bridge fee for a transfer.
   * @param {{ from_chain: 'ethereum'|'bsc'|'polygon', amount: number }} params
   */
  async estimateFee(params) {
    return this._fetch('/bridge/fee', {
      method: 'POST',
      body: JSON.stringify(params),
    });
  }
}
