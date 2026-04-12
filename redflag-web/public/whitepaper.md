# redflag.web3 — Technical Whitepaper

**Version 1.0 · April 2026**

---

## Abstract

redflag.web3 is a Layer-1 blockchain that implements NIST-standardized post-quantum cryptography throughout its entire stack. It combines the Bullshark DAG Byzantine Fault Tolerant consensus protocol with ML-DSA-65 digital signatures and ML-KEM-768 key encapsulation to provide security against both classical and quantum adversaries. The network includes a native AMM DEX, a threshold-signature cross-chain bridge, on-chain governance, and a proof-of-stake validator system.

---

## 1. Motivation

Current blockchain networks rely on ECDSA or EdDSA signatures and ECDH key exchange — all of which are broken by Shor's algorithm running on a sufficiently powerful quantum computer. NIST completed its post-quantum cryptography standardization in 2024 (FIPS 203, 204, 205), providing production-ready replacements. redflag.web3 is the first blockchain to deploy these standards as its primary cryptographic layer, not as an optional upgrade.

---

## 2. Cryptographic Primitives

### 2.1 ML-DSA-65 (FIPS 204)
All transaction signatures use ML-DSA-65, a lattice-based digital signature scheme from the CRYSTALS-Dilithium family.

- **Security level:** NIST Level 3 (equivalent to AES-192)
- **Public key size:** 1,952 bytes
- **Signature size:** 3,293 bytes
- **Signing:** deterministic (no random nonce required)
- **Address format:** hex encoding of the 1,952-byte public key

Every transaction is signed by the sender's ML-DSA-65 key. Verification happens at the state execution layer before any balance change is applied.

### 2.2 ML-KEM-768 (FIPS 203)
Node-to-node communication uses ML-KEM-768 (CRYSTALS-Kyber) for key encapsulation.

- **Security level:** NIST Level 3
- **Ciphertext size:** 1,088 bytes
- **Shared secret size:** 32 bytes

The threshold mempool uses ML-KEM-768 to establish per-round encryption keys between committee members, preventing transaction content from being observed during the ordering phase.

### 2.3 Hybrid Key Exchange
The P2P layer uses a hybrid X25519 + ML-KEM-768 scheme: both a classical and a post-quantum shared secret are derived and combined via HKDF. This preserves security against classical attackers even if the quantum algorithm has an unknown weakness.

---

## 3. Consensus: Bullshark DAG

### 3.1 DAG Structure
Validators submit signed *vertices* to a Directed Acyclic Graph (DAG). Each vertex contains:
- A set of transactions
- References (edges) to at least 2f+1 vertices from the previous round
- An ML-DSA-65 signature from the proposing validator

Rounds proceed every ~200ms. There is no leader election — all validators propose simultaneously, eliminating single-point-of-failure bottlenecks.

### 3.2 Ordering
The Bullshark protocol deterministically orders committed vertices using the DAG structure itself as the communication substrate. A vertex is committed when it is reachable (directly or transitively) by 2f+1 validators in two subsequent anchor rounds.

### 3.3 Safety and Liveness
- **Safety:** If 2f+1 validators are honest, no two honest validators commit conflicting vertices.
- **Liveness:** If the network is partially synchronous, committed vertices are finalized in O(f) rounds.
- **Fault tolerance:** With n=5 nodes, tolerates f=1 Byzantine failure.

---

## 4. Token: RF

| Parameter | Value |
|-----------|-------|
| Symbol | RF |
| Decimals | 6 |
| Total supply cap | 2,100,000,000 RF |
| Genesis supply | 1,500,000,000 RF |
| Chain ID | 2100 |
| Min transaction fee | 1 RF (anti-spam) |
| Min validator stake | 10,000 RF |

### 4.1 Fee Distribution
All transaction fees accumulate in `RedFlag_Protocol_FeePool`. At the end of each committed DAG round, the fee pool is distributed proportionally to validators by their staked RF.

### 4.2 Replay Protection
Every transaction includes:
- `nonce`: must equal the sender account's current nonce (strictly sequential)
- `chain_id`: must equal 2100 (cross-chain replay impossible)

Both fields are covered by the ML-DSA-65 signature.

---

## 5. Native DEX

The built-in AMM DEX uses the constant-product formula **x · y = k**.

### 5.1 Trading Pairs
Nine pairs are available at genesis, all against RF:
wETH · wBNB · wMATIC · wSOL · wAVAX · wARB · wBTC · wUSDC · wUSDT

### 5.2 Fee Model
- Swap fee: **0.3%** of input amount
- Fees stay in the pool, increasing LP token value over time
- No protocol fee at launch (governance can activate)

### 5.3 Liquidity Positions
LP tokens are minted proportional to the deposited value. The first liquidity provider sets the initial price. Subsequent providers must deposit at the current ratio.

### 5.4 Price Oracle
The pool records `(timestamp, price)` tuples after every swap. External contracts can use this time-weighted price history as a manipulation-resistant oracle.

---

## 6. Cross-Chain Bridge

### 6.1 Architecture
The bridge connects redflag.web3 to EVM chains (Ethereum, BSC, Polygon) using a threshold signature committee.

**Lock & Mint (EVM → redflag.web3):**
1. User locks native tokens in an EVM smart contract
2. Bridge relayer observes the lock event
3. Committee of 3 bridge nodes each sign a mint authorization (ML-DSA-65)
4. When 2-of-3 signatures are collected, wTokens are minted on redflag.web3

**Burn & Release (redflag.web3 → EVM):**
1. User burns wTokens via `POST /bridge/burn`
2. Committee signs a release authorization
3. Relayer submits release transaction on the EVM chain

### 6.2 Security Properties
- **Threshold:** 2-of-3 — compromising one bridge node is insufficient
- **Replay protection:** Each bridge event uses a unique nonce
- **No admin keys:** The bridge committee is defined at deployment; no single party can upgrade it

---

## 7. Staking & Validators

Validators must stake ≥ 10,000 RF. Staked RF is locked in `RedFlag_Protocol_Stake_v1`.

**Validator selection:** All staked validators participate in consensus. There is currently no slashing; penalties will be introduced in a future governance proposal.

**Rewards:** Fee pool + 1 RF base reward per committed vertex + 0.1 RF per transaction included.

---

## 8. Governance

On-chain governance allows any staked address to submit proposals. Proposals go through three stages:

1. **Submission:** Proposer deposits 100 RF (refunded if proposal passes)
2. **Voting period:** 7 days. Votes are weighted by staked RF
3. **Execution:** If YES > 50% of participating stake, proposal is executed on-chain

Governance can modify: fee rates, minimum stake, bridge committee members, token supply parameters.

---

## 9. Network

### 9.1 P2P Layer
Built on **libp2p** with:
- TCP + QUIC transports
- Noise protocol (hybrid classical + ML-KEM-768) for session encryption
- Gossipsub for block/transaction propagation
- Kademlia DHT for peer discovery

### 9.2 Bootstrap
New nodes fetch the bootstrap peer list from `GET /network/addrs` on a known node. This returns the node's private IP multiaddresses (preferred for internal network communication on the same hosting provider) and public addresses as fallback.

---

## 10. Roadmap

| Phase | Target | Milestone |
|-------|--------|-----------|
| Testnet | Q2 2026 | 5-node network, DEX, bridge live |
| Audit | Q3 2026 | External audit (Certik / Trail of Bits) |
| Mainnet | Q4 2026 | Public validator onboarding, bridge mainnet |
| Ecosystem | 2027 | WASM smart contracts, mobile wallet |

---

## 11. References

1. NIST FIPS 204 — Module-Lattice-Based Digital Signature Standard (ML-DSA)
2. NIST FIPS 203 — Module-Lattice-Based Key-Encapsulation Mechanism Standard (ML-KEM)
3. Spiegelman et al. — "Bullshark: DAG BFT Protocols Made Practical" (2022)
4. Uniswap v2 Whitepaper — Constant Product AMM
5. libp2p Specification — https://github.com/libp2p/specs

---

*redflag.web3 · Chain ID 2100 · https://x.com/franff546758 · https://t.me/redflag21blockchain*
