/**
 * @redflag/sdk
 * Official SDK for building apps on the RedFlag Network.
 *
 * Chain ID: 2100 | Consensus: Bullshark DAG
 * Crypto: ML-DSA-65 + ML-KEM-768 (post-quantum) | TPS: ~10,000
 *
 * Quick start:
 *
 *   import { createSDK } from '@redflag/sdk'
 *
 *   const { chain, dex, staking, bridge } = createSDK()
 *
 *   const balance = await chain.getBalance('your-rf-address')
 *   const pools   = await dex.getPools()
 *   const quote   = await dex.getQuote({ pool_id: 'RF_wETH', input_token: 'RF', amount_in: 1_000_000 })
 *   const status  = await bridge.getStatus()
 */

export { RedflagClient, RF_MAINNET, RF_NODE2, RF_NODE3, CHAIN_ID, DECIMALS, SYMBOL } from './client.js';
export { DexClient }     from './dex.js';
export { StakingClient } from './staking.js';
export { BridgeClient, BRIDGE_URL, SUPPORTED_CHAINS } from './bridge.js';
export {
  fromRawRF, toRawRF, formatRF,
  isValidAddress,
  RF_CHAIN,
  ADD_RF_NETWORK_PARAMS,
} from './wallet.js';

import { RedflagClient } from './client.js';
import { DexClient }     from './dex.js';
import { StakingClient } from './staking.js';
import { BridgeClient }  from './bridge.js';

/**
 * Create a fully wired SDK instance with all modules.
 *
 * @param {string} [nodeUrl] - RF RPC URL (default: mainnet node1)
 * @returns {{ chain: RedflagClient, dex: DexClient, staking: StakingClient, bridge: BridgeClient }}
 *
 * @example
 * import { createSDK } from '@redflag/sdk'
 * const { chain, dex, staking, bridge } = createSDK()
 * const info   = await chain.getChainInfo()
 * const pools  = await dex.getPools()
 */
export function createSDK(nodeUrl) {
  const chain   = new RedflagClient(nodeUrl);
  const dex     = new DexClient(chain);
  const staking = new StakingClient(chain);
  const bridge  = new BridgeClient();
  return { chain, dex, staking, bridge };
}
