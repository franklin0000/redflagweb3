/**
 * @redflag/sdk — DexClient
 * Interact with the RedFlag Network native DEX (AMM, constant-product).
 *
 * Pools available:
 *   RF_wETH  — RedFlag ↔ Wrapped Ethereum
 *   RF_wBNB  — RedFlag ↔ Wrapped BNB
 *   RF_wMATIC — RedFlag ↔ Wrapped MATIC
 */

export class DexClient {
  constructor(rfClient) {
    this._rf = rfClient;
  }

  // ── Pools ────────────────────────────────────────────────────────────────

  /** List all liquidity pools */
  async getPools() {
    return this._rf._get('/dex/pools');
  }

  /**
   * Get a single pool by ID (e.g. "RF_wETH")
   * @param {string} poolId
   */
  async getPool(poolId) {
    return this._rf._get(`/dex/pool/${poolId}`);
  }

  /**
   * Get price history for a pool
   * @param {string} poolId
   */
  async getPriceHistory(poolId) {
    return this._rf._get(`/dex/pool/${poolId}/prices`);
  }

  /**
   * Get OHLCV history for a pool
   * @param {string} poolId
   */
  async getOhlcvHistory(poolId) {
    return this._rf._get(`/dex/pool/${poolId}/history`);
  }

  // ── Quotes ───────────────────────────────────────────────────────────────

  /**
   * Get a swap quote (no execution).
   * @param {{ pool_id: string, input_token: string, amount_in: number }} params
   * @returns {{ amount_out: number, price_impact: number, fee: number }}
   */
  async getQuote(params) {
    return this._rf._post('/dex/quote', params);
  }

  // ── Swap ─────────────────────────────────────────────────────────────────

  /**
   * Execute a token swap.
   * @param {{
   *   pool_id: string,
   *   input_token: string,
   *   amount_in: number,
   *   min_amount_out: number,
   *   sender: string,
   *   signature: string,
   *   nonce: number
   * }} params
   */
  async swap(params) {
    return this._rf._post('/dex/swap', params);
  }

  // ── Liquidity ─────────────────────────────────────────────────────────────

  /**
   * Add liquidity to a pool.
   * @param {{
   *   pool_id: string,
   *   amount_rf: number,
   *   amount_token: number,
   *   sender: string,
   *   signature: string,
   *   nonce: number
   * }} params
   */
  async addLiquidity(params) {
    return this._rf._post('/dex/liquidity/add', params);
  }

  /**
   * Remove liquidity from a pool.
   * @param {{
   *   pool_id: string,
   *   lp_amount: number,
   *   sender: string,
   *   signature: string,
   *   nonce: number
   * }} params
   */
  async removeLiquidity(params) {
    return this._rf._post('/dex/liquidity/remove', params);
  }

  /**
   * Get LP position for an address in a pool.
   * @param {string} address
   * @param {string} poolId
   */
  async getPosition(address, poolId) {
    return this._rf._get(`/dex/position/${address}/${poolId}`);
  }

  // ── Helpers ───────────────────────────────────────────────────────────────

  /**
   * Calculate minimum output with slippage tolerance.
   * @param {number} expectedOut  - Expected amount out (from getQuote)
   * @param {number} slippagePct  - Max slippage in % (e.g. 0.5 for 0.5%)
   */
  static withSlippage(expectedOut, slippagePct = 0.5) {
    return Math.floor(expectedOut * (1 - slippagePct / 100));
  }
}
