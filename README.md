# redflag.web3 — Post-Quantum Blockchain

> Chain ID: 2100 · ML-DSA-65 · ML-KEM-768 · Bullshark DAG · Native DEX

[![Network](https://img.shields.io/badge/network-testnet-orange)](https://redflagweb3-node1.onrender.com/status)
[![Nodes](https://img.shields.io/badge/nodes-5-green)](https://redflagweb3-node1.onrender.com/validators)
[![Twitter](https://img.shields.io/badge/Twitter-%40franff546758-black)](https://x.com/franff546758)
[![Telegram](https://img.shields.io/badge/Telegram-redflag21blockchain-blue)](https://t.me/redflag21blockchain)

---

## What is redflag.web3?

redflag.web3 is a Layer-1 blockchain built in Rust with **post-quantum cryptography** — resistant to attacks from both classical and quantum computers.

| Feature | Details |
|---------|---------|
| **Consensus** | Bullshark DAG (BFT, ~200ms finality) |
| **Signatures** | ML-DSA-65 (FIPS 204 — NIST post-quantum standard) |
| **Key Exchange** | ML-KEM-768 (FIPS 203 — NIST post-quantum standard) |
| **Native DEX** | AMM constant-product, 9 trading pairs |
| **Bridge** | Threshold 2-of-3 multi-sig to Ethereum / BSC / Polygon |
| **Governance** | On-chain proposals + staked voting |
| **Chain ID** | 2100 |
| **Token** | RF (6 decimals) |
| **Block time** | ~200ms |

---

## Live Network

| Service | URL |
|---------|-----|
| Dashboard (PWA) | https://ipfs.io/ipfs/QmSC5VEFHRWT1XTBohtyWwhRMyYMdupwViw8CztgQQfr5U/ |
| Node 1 (RPC) | https://redflagweb3-node1.onrender.com |
| Explorer | Dashboard → Explorer tab |
| Market API | https://redflagweb3-node1.onrender.com/api/v1/ticker |

---

## Run a Node (1 command)

```bash
curl -sSf https://redflagweb3-node1.onrender.com/install.sh | bash
```

Requirements: Linux or macOS, Rust (auto-installed if missing).

### Manual

```bash
git clone https://github.com/franklin0000/redflagweb3
cd redflagweb3
cargo build --release -p redflag-network

DATA_DIR=~/.redflag/data \
BOOTSTRAP_URL=https://redflagweb3-node1.onrender.com/network/addrs \
PORT=8545 P2P_PORT=9000 \
./target/release/redflag-network
```

---

## Become a Validator

1. Run a node
2. Get your node address from the dashboard
3. Stake ≥ 10,000 RF to `RedFlag_Protocol_Stake_v1`
4. Register: `POST https://redflagweb3-node1.onrender.com/validators/apply`

---

## Market Data API (CoinGecko compatible)

```
GET /api/v1/ticker     — all trading pairs
GET /api/v1/summary    — 24h summary per pair
GET /api/v1/orderbook  — AMM order book simulation
GET /api/v1/trades     — recent swap history
GET /api/v1/assets     — asset metadata
```

**9 pairs:** RF/wETH · RF/wBNB · RF/wMATIC · RF/wSOL · RF/wAVAX · RF/wARB · RF/wBTC · RF/wUSDC · RF/wUSDT

---

## Architecture

```
redflag-network/   ← P2P node + RPC server (main binary)
redflag-consensus/ ← Bullshark DAG + threshold mempool
redflag-state/     ← State DB: accounts, DEX, staking, governance
redflag-crypto/    ← ML-DSA-65, ML-KEM-768, hybrid key exchange
redflag-bridge/    ← EVM bridge relayer (Ethereum / BSC / Polygon)
redflag-vm/        ← Smart contract VM (WASM)
redflag-core/      ← Types, constants, transaction format
redflag-web/       ← React PWA dashboard
redflag-sdk/       ← JavaScript SDK (@redflag/sdk)
```

---

## Security

See [SECURITY.md](./SECURITY.md) for the full internal audit report.

- Signatures: ML-DSA-65 (NIST FIPS 204)
- P2P encryption: ML-KEM-768 (NIST FIPS 203)
- Bridge: threshold 2-of-3 ML-DSA committee

Report vulnerabilities via [Telegram](https://t.me/redflag21blockchain) before public disclosure.

---

## Community

| | |
|-|-|
| 𝕏 Twitter | [@franff546758](https://x.com/franff546758) |
| Telegram | [redflag21blockchain](https://t.me/redflag21blockchain) |

---

MIT License
