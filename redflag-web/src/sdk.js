import axios from 'axios';

class RedFlagSDK {
    constructor(nodeUrl = null) {
        this.nodeUrl = nodeUrl || import.meta.env.VITE_NODE_URL || window.location.origin;
        this.wsUrl = this.nodeUrl.replace(/^http/, 'ws') + '/ws';
        this.ws = null;
        this.listeners = new Set();
    }

    // ── WebSocket ──
    connectWS(onMessage) {
        if (this.ws) this.ws.close();
        this.ws = new WebSocket(this.wsUrl);
        
        this.ws.onmessage = (event) => {
            try {
                const data = JSON.parse(event.data);
                this.listeners.forEach(fn => fn(data));
                if (onMessage) onMessage(data);
            } catch (e) { console.error("WS Parse Error", e); }
        };

        this.ws.onclose = () => {
            console.log("WS Closed. Reconnecting...");
            setTimeout(() => this.connectWS(onMessage), 3000);
        };
        
        return () => this.ws.close();
    }

    subscribe(fn) {
        this.listeners.add(fn);
        return () => this.listeners.delete(fn);
    }

    // ── REST API ──
    async getStatus()        { return (await axios.get(`${this.nodeUrl}/status`)).data; }
    async getNetworkInfo()   { return (await axios.get(`${this.nodeUrl}/network-info`)).data; }
    async getNetworkStats()  { return (await axios.get(`${this.nodeUrl}/network/stats`)).data; }
    async getBalance(addr)   { return addr ? (await axios.get(`${this.nodeUrl}/balance/${addr}`)).data : { balance: 0, nonce: 0 }; }
    async getAccount(addr)   { return addr ? (await axios.get(`${this.nodeUrl}/account/${addr}`)).data : null; }
    async getHistory(addr)   { return addr ? (await axios.get(`${this.nodeUrl}/history/${addr}`)).data.history : []; }
    async getVertices()      { return (await axios.get(`${this.nodeUrl}/dag/vertices`)).data; }
    async getSummary()       { return (await axios.get(`${this.nodeUrl}/dag/summary`)).data; }
    async getMempool()       { return (await axios.get(`${this.nodeUrl}/mempool`)).data; }
    async getRoundEk()       { return (await axios.get(`${this.nodeUrl}/round-ek`)).data; }
    
    // Explorer
    async search(query)      { return (await axios.get(`${this.nodeUrl}/explorer/search/${query}`)).data; }
    async getTx(hash)        { return (await axios.get(`${this.nodeUrl}/explorer/tx/${hash}`)).data; }

    // ── DEX ──
    async getDexPools()      { return (await axios.get(`${this.nodeUrl}/dex/pools`)).data; }
    async getDexPool(id)     { return (await axios.get(`${this.nodeUrl}/dex/pool/${id}`)).data; }
    async getDexHistory(id)  { return (await axios.get(`${this.nodeUrl}/dex/pool/${id}/history`)).data; }
    async getDexPrices(id)   { return (await axios.get(`${this.nodeUrl}/dex/pool/${id}/prices`)).data; }
    async getDexPosition(addr,pool) { return (await axios.get(`${this.nodeUrl}/dex/position/${addr}/${pool}`)).data; }
    async dexQuote(pool_id, direction, amount_in) {
        return (await axios.post(`${this.nodeUrl}/dex/quote`, { pool_id, direction, amount_in: parseInt(amount_in) })).data;
    }
    async dexSwap(private_key_hex, pool_id, direction, amount_in, min_amount_out=0) {
        return (await axios.post(`${this.nodeUrl}/dex/swap`, {
            private_key_hex, pool_id, direction,
            amount_in: parseInt(amount_in),
            min_amount_out: parseInt(min_amount_out),
        })).data;
    }
    async dexAddLiquidity(private_key_hex, pool_id, amount_rf, amount_b) {
        return (await axios.post(`${this.nodeUrl}/dex/liquidity/add`, {
            private_key_hex, pool_id,
            amount_rf: parseInt(amount_rf),
            amount_b: parseInt(amount_b),
        })).data;
    }
    async dexRemoveLiquidity(private_key_hex, pool_id, lp_tokens) {
        return (await axios.post(`${this.nodeUrl}/dex/liquidity/remove`, {
            private_key_hex, pool_id, lp_tokens: parseInt(lp_tokens),
        })).data;
    }

    // ── Generic HTTP ──
    async get(path)        { return (await axios.get(`${this.nodeUrl}${path}`)).data; }
    async post(path, body) { return (await axios.post(`${this.nodeUrl}${path}`, body)).data; }

    // ── Staking ──
    async getStakingInfo()   { return (await axios.get(`${this.nodeUrl}/staking/info`)).data; }
    async getStakes()        { return (await axios.get(`${this.nodeUrl}/staking/stakes`)).data; }
    async stakingStake(private_key_hex, amount) {
        return (await axios.post(`${this.nodeUrl}/staking/stake`, { private_key_hex, amount: parseInt(amount) })).data;
    }
    async stakingUnstake(private_key_hex) {
        return (await axios.post(`${this.nodeUrl}/staking/unstake`, { private_key_hex })).data;
    }
    async stakingWithdraw(private_key_hex) {
        return (await axios.post(`${this.nodeUrl}/staking/withdraw`, { private_key_hex })).data;
    }
    async stakingRewards(address) {
        return (await axios.get(`${this.nodeUrl}/staking/rewards/${address}`)).data;
    }

    // ── Wallet API ──
    async walletNew() {
        // En una fase Pro, esto solo devuelve la configuración inicial.
        const res = await axios.post(`${this.nodeUrl}/wallet/new`, {});
        return res.data;
    }

    async walletSend(private_key_hex, receiver, amount, fee = 1) {
        const res = await axios.post(`${this.nodeUrl}/wallet/send`, {
            private_key_hex,
            receiver,
            amount: parseInt(amount),
            fee: parseInt(fee),
        });
        return res.data;
    }

    async walletFaucet(address, amount = 1000) {
        const res = await axios.post(`${this.nodeUrl}/wallet/faucet`, {
            address,
            amount: parseInt(amount),
        });
        return res.data;
    }
}

export const sdk = new RedFlagSDK();
export default sdk;
