/**
 * @redflag/sdk — StakingClient
 * Staking operations on the RedFlag Network.
 */

export class StakingClient {
  constructor(rfClient) {
    this._rf = rfClient;
  }

  /** Global staking info: total staked, APY, validator count */
  async getInfo() {
    return this._rf._get('/staking/info');
  }

  /**
   * Get all stakes for an address.
   * @param {string} address
   */
  async getStakes(address) {
    const data = await this._rf._get(`/staking/stakes`);
    if (address) {
      return Array.isArray(data)
        ? data.filter(s => s.validator === address || s.delegator === address)
        : data;
    }
    return data;
  }

  /**
   * Stake RF tokens as a validator.
   * @param {{
   *   address: string,
   *   amount: number,
   *   signature: string,
   *   nonce: number
   * }} params
   */
  async stake(params) {
    return this._rf._post('/staking/stake', params);
  }

  /**
   * Unstake RF tokens.
   * @param {{
   *   address: string,
   *   amount: number,
   *   signature: string,
   *   nonce: number
   * }} params
   */
  async unstake(params) {
    return this._rf._post('/staking/unstake', params);
  }
}
