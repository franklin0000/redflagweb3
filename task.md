# RedFlag 2.1 — Estado Real del Proyecto (Actualizado 2026-04-11)

## ✅ COMPLETO — Todo funciona, 10/10 tests pasan

### Core & Criptografía ✅
- [x] Cargo Workspace (core, crypto, network, consensus, rpc, state, vm, bridge, cli, web)
- [x] `redflag-crypto` — ML-DSA-65 (firma post-cuántica) + ML-KEM-768 (encapsulación)
- [x] Handshake híbrido PQC: X25519 (clásico) + ML-KEM (cuántico) → HybridSecret
- [x] Tests: `test_hybrid_flow`, `test_signing_flow`, `test_hybrid_handshake_integration`

### Core & Transacciones ✅
- [x] `Transaction` con read_set/write_set para ejecución paralela
- [x] `EncryptedTransaction` — threshold mempool cifrado (anti-MEV)
- [x] Detección de conflictos y agrupación paralela (Rayon)
- [x] Chain ID 2100, nonce, replay protection, fee mínimo

### Consenso Bullshark DAG ✅
- [x] `Dag` — estructura de vértices y certificados
- [x] `Mempool` — transacciones normales y cifradas
- [x] `ConsensusEngine` — Bullshark BFT, order_transactions, commit recursivo
- [x] Persistencia completa con Sled (DAG + estado + metadata)
- [x] ThresholdMempool — cifrado/descifrado ML-KEM por ronda
- [x] Rotación de llaves de threshold por ronda
- [x] Test: `test_dag_persistence_on_reboot`, `test_threshold_encrypt_decrypt`, etc.

### Red P2P ✅
- [x] libp2p 0.53: TCP + QUIC, Gossipsub, Kademlia DHT, mDNS, Identify
- [x] Handshake PQC híbrido al conectar peers
- [x] DAG sync: GetVertex, GetCertificate, SyncFrom (batch), GetValidatorKey, Ping
- [x] Parent-fetching automático de vértices faltantes
- [x] Bootstrap peer via `BOOTSTRAP_PEER` env var
- [x] Test: `test_dag_sync_parent_fetching`

### Estado & DEX ✅
- [x] `StateDB` con cuentas, historial de TXs, índice por hash
- [x] Ejecución paralela de TXs no-conflictivas (Rayon)
- [x] DEX AMM (x·y=k, fee 0.3%): pools RF/wETH, RF/wBNB, RF/wMATIC
- [x] Token ledger multi-token (wETH, wBNB, wMATIC)
- [x] Fee pool acumulado en `RedFlag_Protocol_FeePool`

### RPC & Dashboard ✅
- [x] Axum HTTP + WebSocket tiempo real
- [x] Rate limiting: 60 req/min por IP
- [x] Faucet con cooldown 24h por dirección
- [x] Endpoints: /status, /balance, /account, /history, /wallet/new, /wallet/send
- [x] DEX endpoints: /dex/pools, /dex/quote, /dex/swap, /dex/liquidity/*
- [x] Bridge endpoints: /bridge/info, /bridge/mint
- [x] Prometheus metrics: /metrics
- [x] Explorer: /explorer/search, /explorer/tx/:hash

### Bridge EVM ✅
- [x] Bridge relayer Rust (redflag-bridge)
- [x] BridgeRF.sol (lock/unlock en EVM)
- [x] Soporte: ETH Sepolia, BSC Testnet, Polygon Amoy

### Infraestructura ✅
- [x] Docker + docker-compose (nodo1 + nodo2 + bridge + nginx + prometheus + grafana)
- [x] Deploy script automatizado (`./deploy/scripts/deploy.sh DOMINIO`)
- [x] SSL Let's Encrypt automático
- [x] Identidad persistente (libp2p keypair + ML-DSA keypair)
- [x] Faucet keypair persistente

---

## 🚀 EN PROGRESO

### Staking de Validadores
- [ ] `StakingState` en `redflag-state` — staking, unstaking, slashing
- [ ] Distribución de fees del fee pool a validadores stakers
- [ ] Endpoints RPC: /staking/stake, /staking/unstake, /staking/rewards
- [ ] Validadores deben tener stake mínimo para participar en consenso

---

## 📋 PRÓXIMAS TAREAS

- [ ] Deploy en producción (Render / VPS)
- [ ] Frontend del nodo (`redflag-web`) — conectar con RPC real
- [ ] Cert aggregation multi-nodo real (2f+1 firmas para commit)
- [ ] CLI completa (`redflag-cli`) — wallet, send, faucet, dex
