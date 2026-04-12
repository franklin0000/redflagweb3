# redflag.web3 — Security Report

> Internal audit · April 2026  
> Scope: redflag-network (RPC + P2P), redflag-state (DEX, staking, tokens), redflag-consensus, redflag-crypto, redflag-bridge  
> Chain ID: 2100 · Testnet only at time of writing

---

## Executive Summary

The codebase implements a post-quantum blockchain node with Bullshark DAG
consensus, ML-DSA-65 signing, ML-KEM-768 key encapsulation, and an AMM DEX.
This review covers the most security-relevant components. No critical
vulnerabilities were found that allow arbitrary fund theft or remote code
execution. Several medium-severity issues and hardening recommendations are
documented below.

---

## Findings

### HIGH

#### H-1 — CORS set to `permissive()` on RPC server

**File:** `redflag-network/src/rpc.rs:227`  
**Risk:** Any origin may issue credentialed JSON-RPC requests against a node
running on a user's localhost (CSRF-style attack via malicious website).

**Impact:** A web page visited by a node operator could send transactions,
drain the faucet, or call `/wallet/new` on their behalf.

**Recommended fix:** Restrict allowed origins to the dashboard domain and
localhost only:
```rust
CorsLayer::new()
    .allow_origin([
        "https://redflagweb3-app.onrender.com".parse::<HeaderValue>().unwrap(),
        "http://localhost:5173".parse::<HeaderValue>().unwrap(),
    ])
    .allow_methods([Method::GET, Method::POST])
    .allow_headers(Any)
```
For public Render-hosted nodes where the API is meant to be public, the
permissive setting is acceptable, but operators running local nodes should
restrict CORS.

**Status:** Known / accepted for testnet. Fix before mainnet.

---

#### H-2 — Private keys transmitted over HTTP in `/wallet/send`

**File:** `redflag-network/src/rpc.rs` — `WalletSendRequest`  
**Risk:** The `/wallet/send` endpoint accepts `private_key_hex` in the POST
body. On plain HTTP (non-TLS Render free tier) or on any MitM path, the
private key is exposed in transit.

**Impact:** Key theft → full account compromise.

**Recommended fix:** All public-facing endpoints must be served behind HTTPS
(Render enforces this automatically for `*.onrender.com`). Document this
requirement. For a mainnet wallet, move signing to the client and only submit
a signed transaction.

**Status:** Mitigated by Render TLS termination. Client-side signing is the
long-term goal.

---

### MEDIUM

#### M-1 — Rate limiting uses in-memory HashMap (no persistence, no per-route caps)

**File:** `redflag-network/src/rpc.rs:326` — `ip_rate_limit`  
**Risk:** Rate-limit counters are in `DashMap<String, (u64, u32)>` which resets
on restart. A client can bypass the limit by triggering a node restart.
Additionally the faucet cooldown and global IP limit share the same 60 req/min
counter regardless of endpoint cost.

**Recommended fix:** Persist counters (e.g., sled) or use a token-bucket per
endpoint. Apply stricter caps to expensive endpoints (`/faucet`, `/dex/swap`).

---

#### M-2 — DEX: division by zero when removing liquidity with zero total_lp

**File:** `redflag-state/src/dex.rs:258`  
```rust
let amount_rf = (pool.reserve_rf as u128 * lp_tokens as u128 / pool.total_lp as u128) as u64;
```
If `total_lp == 0` (corrupt or empty pool) this is integer division by zero
→ `panic!` in release builds (Rust divides by zero = panic, not wrap).

**Recommended fix:**
```rust
if pool.total_lp == 0 { anyhow::bail!("Pool vacío — total_lp es 0"); }
```

---

#### M-3 — Bridge legacy secret mode accepts short secrets (timing-safe comparison mitigates but doesn't prevent brute-force)

**File:** `redflag-network/src/rpc.rs:970-974`  
The constant-time comparison is correct. However the code allows any non-empty,
non-`bridge_dev_secret` value in `BRIDGE_MINT_SECRET`. A short or low-entropy
secret (e.g., `"1234"`) passes the check.

**Recommended fix:** Enforce minimum 32-character random secret at startup:
```rust
if expected.len() < 32 {
    eprintln!("FATAL: BRIDGE_MINT_SECRET demasiado corto (mínimo 32 chars)");
    std::process::exit(1);
}
```

---

#### M-4 — `listen_addrs` exposed unauthenticated via `/network/addrs`

**File:** `redflag-network/src/rpc.rs:438`  
**Risk:** Private RFC-1918 IP addresses (10.x.x.x, 172.x.x.x) of the node's
internal network are publicly accessible. This aids network mapping of Render's
private mesh.

**Impact:** Low for testnet; medium for production where infrastructure topology
should be private.

**Recommended fix:** Restrict `/network/addrs` to authenticated node-to-node
calls (add a bearer token or internal-only route) before mainnet.

---

### LOW

#### L-1 — `unwrap()` calls on RwLock reads

**File:** `redflag-network/src/rpc.rs:439`  
```rust
let addrs = state.listen_addrs.read().unwrap().clone();
```
If the lock is poisoned (writer panicked), subsequent reads panic. Prefer:
```rust
let addrs = state.listen_addrs.read().unwrap_or_else(|e| e.into_inner()).clone();
```

---

#### L-2 — ~~Transaction replay possible cross-chain~~ (RESOLVED — not a vulnerability)

**File:** `redflag-state/src/lib.rs:200-203`  
Verification serializes the full `Transaction` struct (minus the signature
field) via `postcard::to_allocvec`, which includes `chain_id`. Cross-chain
replay is therefore impossible — a signature produced for Chain ID 2100 will
fail verification on any other chain using a different ID. No action needed.

---

#### L-3 — Governance proposals allow empty `description`

**File:** `redflag-state/src/governance.rs`  
No minimum length or content validation on `description`. Low risk but enables
spam proposals.

---

#### L-4 — Faucet address is predictable (derived from saved key)

The faucet keypair is deterministic from the file at `{data_dir}/faucet.key`.
If an attacker obtains the file, they control 500 M RF. In production, the
faucet balance should be tightly capped and the key stored in a secrets manager.

---

## Positive Security Observations

| Area | Status |
|------|--------|
| ML-DSA-65 for transaction signing | ✅ Post-quantum safe |
| ML-KEM-768 for P2P key exchange | ✅ Post-quantum safe |
| DEX AMM uses `u128` intermediate to prevent overflow | ✅ |
| `checked_add` in `add_liquidity` for reserve accounting | ✅ |
| Constant-time comparison for bridge secret | ✅ |
| Bridge rejects default dev secret in production | ✅ |
| IP rate limiting on faucet and send endpoints | ✅ |
| Faucet cooldown per address (24-hour lockout) | ✅ |
| Threshold signature for bridge mint (preferred over legacy secret) | ✅ |
| Transaction nonce prevents replay within same chain | ✅ |
| Address validation (64-char hex) before faucet/send | ✅ |

---

## Audit Recommendations for External Firms

The following areas should receive the most attention from external auditors
(Certik / Trail of Bits / OtterSec):

1. **Bullshark DAG consensus safety** — Byzantine fault tolerance under 5-node
   testnet, equivocation handling, fork resolution.
2. **Threshold cryptography** — ML-KEM-768 committee key establishment,
   signature aggregation in `threshold.rs`, replay protection on threshold
   approvals.
3. **Bridge relay logic** — `redflag-bridge/src/relayer.rs` and `evm.rs`:
   double-mint protection, event replay, signature ordering.
4. **DEX economic safety** — sandwich attack surface on AMM, large-trade
   slippage, LP manipulation via flash-equivalent multi-tx.
5. **State serialization** — `postcard` deserialization of untrusted peer data
   in `sync/state` — verify no deserializer panics on crafted payloads.

---

## Disclosure Policy

Security issues should be reported privately to the project maintainers before
public disclosure. A bug bounty program will be announced alongside the mainnet
launch.
