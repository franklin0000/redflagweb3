# CoinGecko Listing Submission — redflag.web3 (RF)

> Submit at: https://www.coingecko.com/en/coins/add  
> Form type: Exchange listing (DEX market data) + Coin listing

---

## 1. Basic Coin Information

| Field | Value |
|-------|-------|
| **Project Name** | redflag.web3 |
| **Ticker / Symbol** | RF |
| **Chain** | redflag.web3 (native, Chain ID 2100) |
| **Token Type** | Native Layer-1 gas & governance token |
| **Launch Date** | 2026 (Testnet) |
| **Total Supply** | 2,100,000,000 RF (2.1 B) |
| **Circulating Supply** | ≈ 1,500,000,000 RF (genesis + faucet) |
| **Decimal Places** | 6 |
| **Contract Address** | N/A — native coin, no ERC-20 contract |
| **Website** | https://ipfs.io/ipfs/QmSC5VEFHRWT1XTBohtyWwhRMyYMdupwViw8CztgQQfr5U/ |
| **Whitepaper / Docs** | https://github.com/franklin0000/redflagweb3/blob/main/README.md |
| **GitHub** | https://github.com/franklin0000/redflagweb3 |

---

## 2. Project Description (for listing page)

redflag.web3 is a post-quantum Layer-1 blockchain built in Rust, featuring:

- **Bullshark DAG consensus** — high-throughput, BFT-safe vertex ordering
- **ML-DSA-65 signatures** — NIST-standardized post-quantum signing
- **ML-KEM-768 key exchange** — quantum-safe P2P encryption
- **AMM DEX** — built-in constant-product market maker (9 trading pairs: wETH, wBNB, wMATIC, wSOL, wAVAX, wARB, wBTC, wUSDC, wUSDT)
- **EVM bridge** — threshold-signature bridge to Ethereum, BNB Chain, Polygon
- **On-chain governance** — proposal + voting with staked RF

The network uses Chain ID 2100 and supports public node operation via a
one-command installer.

---

## 3. Market Data API (for Exchange / DEX listing)

CoinGecko requires exchange APIs at the format below. All endpoints are live
on the public testnet node.

**Base URL:** `https://redflagweb3-node1.onrender.com`

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/summary` | All trading pairs summary |
| `GET /api/v1/ticker` | Ticker data per pair |
| `GET /api/v1/orderbook?market_pair=RF_wETH` | Order book (AMM simulated) |
| `GET /api/v1/trades?market_pair=RF_wETH` | Recent trade history |
| `GET /api/v1/assets` | Asset metadata |

**Example ticker response:**
```json
{
  "timestamp": 1744000000,
  "tickers": {
    "RF_wETH": {
      "base_id": "RF",
      "quote_id": "wETH",
      "base_name": "RedFlag",
      "quote_name": "Wrapped Ether",
      "base_symbol": "RF",
      "quote_symbol": "wETH",
      "last": 0.0031,
      "bid": 0.0031,
      "ask": 0.0031,
      "volume": 125000.0,
      "isFrozen": "0"
    },
    ...
  }
}
```

All 9 pairs (RF_wETH, RF_wBNB, RF_wMATIC, RF_wSOL, RF_wAVAX, RF_wARB,
RF_wBTC, RF_wUSDC, RF_wUSDT) are always returned.

---

## 4. Social Links

| Platform | URL |
|----------|-----|
| Twitter / X | https://x.com/franff546758 |
| Telegram | https://t.me/redflag21blockchain |
| GitHub | https://github.com/franklin0000/redflagweb3 |
| Website | https://ipfs.io/ipfs/QmSC5VEFHRWT1XTBohtyWwhRMyYMdupwViw8CztgQQfr5U/ |
| Node installer | `curl -sSf https://redflagweb3-node1.onrender.com/install.sh | bash` |
| Explorer | https://ipfs.io/ipfs/QmSC5VEFHRWT1XTBohtyWwhRMyYMdupwViw8CztgQQfr5U/ (tab: Explorer) |
| Validator registration | `POST https://redflagweb3-node1.onrender.com/validators/apply` |

---

## 5. Logo

- **URL:** https://ipfs.io/ipfs/QmSC5VEFHRWT1XTBohtyWwhRMyYMdupwViw8CztgQQfr5U//logo.png
- **Format:** PNG, transparent background
- **Required sizes:** 200x200px (submit separately as file upload)

---

## 6. Exchange Listing (DEX section)

**Exchange Name:** redflag.web3 DEX  
**Exchange Type:** DEX (AMM)  
**Pair count:** 9  
**Liquidity mechanism:** Constant-product AMM (x · y = k)  
**Fee:** 0.3% per swap  

API complies with CoinGecko DEX Data Standard v2:
- `/api/v1/ticker` ✅
- `/api/v1/summary` ✅  
- `/api/v1/orderbook` ✅
- `/api/v1/trades` ✅
- `/api/v1/assets` ✅

---

## 7. Submission Checklist

- [ ] Submit coin at https://www.coingecko.com/en/coins/add
- [ ] Submit exchange at https://www.coingecko.com/en/exchanges/add
- [ ] Upload 200x200 logo PNG
- [ ] Fill project description (use section 2 above)
- [ ] Paste API base URL: `https://redflagweb3-node1.onrender.com`
- [ ] Verify ticker endpoint returns all 9 pairs before submitting
- [ ] Add Twitter/X handle once account is created
- [ ] Add Telegram community link once created
- [ ] Monitor CoinGecko review (typically 30–90 days for new chains)

---

## 8. Verify Endpoints Before Submission

```bash
# Check ticker has all 9 pairs
curl -s https://redflagweb3-node1.onrender.com/api/v1/ticker | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(list(d['tickers'].keys()))"

# Check assets
curl -s https://redflagweb3-node1.onrender.com/api/v1/assets | python3 -m json.tool

# Check summary
curl -s https://redflagweb3-node1.onrender.com/api/v1/summary | python3 -m json.tool
```
