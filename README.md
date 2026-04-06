# redflag.web3 — Post-Quantum Blockchain

**Producción lista. Multi-chain. DEX nativo.**

redflag.web3 es una blockchain de alto rendimiento con consenso Bullshark DAG, criptografía post-cuántica (ML-DSA-65 + ML-KEM-768), DEX nativo AMM y bridge cross-chain a Ethereum, BSC y Polygon.

---

## Inicio Rápido (Producción)

### 1. Requisitos
```bash
# Ubuntu 22.04 LTS en un VPS (mínimo 4GB RAM, 2 CPU, 40GB SSD)
apt update && apt install -y docker.io docker-compose-plugin nginx certbot python3-certbot-nginx
```

### 2. Clonar y configurar
```bash
git clone https://github.com/redflag/redflag2.1
cd redflag2.1
cp .env.production.example .env.production
nano .env.production   # Edita tu dominio y API keys
```

### 3. Deploy completo (1 comando)
```bash
./deploy/scripts/deploy.sh TU_DOMINIO.COM
# Con monitoring (Prometheus + Grafana):
./deploy/scripts/deploy.sh TU_DOMINIO.COM --with-monitoring
```

Esto automaticamente:
- Obtiene certificado SSL con Let's Encrypt
- Configura nginx con rate limiting
- Construye y despliega los contenedores Docker
- Inicia nodos validadores + bridge relayer
- Configura auto-renovación SSL

### 4. Desplegar contratos bridge en EVM
```bash
# Requiere Foundry: curl -L https://foundry.paradigm.xyz | bash
./deploy/scripts/deploy_contracts.sh 0xTU_DIRECCION_ETH
# Actualiza los contratos desplegados en .env.production
# Reinicia el bridge:
docker compose restart bridge
```

---

## Desarrollo Local

```bash
# Backend (Rust)
cargo build --release
DATA_DIR=./node1 PORT=8545 P2P_PORT=9000 ./target/release/redflag-network

# Frontend (React)
cd redflag-web && npm install && npm run dev

# Bridge (en otra terminal)
cd redflag-bridge
ETH_SEPOLIA_RPC=https://... cargo run --release
```

---

## Arquitectura

```
┌─────────────────────────────────────────────────────────────┐
│                    Internet / Users                          │
└──────────────────────┬──────────────────────────────────────┘
                       │ HTTPS (nginx + Let's Encrypt)
┌──────────────────────▼──────────────────────────────────────┐
│                    Nginx Reverse Proxy                       │
│   Rate limiting · SSL termination · WebSocket proxy         │
└──────┬───────────────────────────────────────┬──────────────┘
       │                                       │
┌──────▼──────┐                         ┌──────▼──────┐
│   Node 1    │◄──── P2P libp2p ───────►│   Node 2    │
│  (Primary)  │   QUIC + TCP + mDNS     │ (Secondary) │
│  Port 8545  │   Gossipsub + Kademlia  │  Port 8546  │
└──────┬──────┘                         └─────────────┘
       │
       │ HTTP (internal)
┌──────▼──────┐
│   Bridge    │◄──── WebSocket ────► Ethereum Sepolia
│  Relayer    │◄──── HTTP RPC  ────► BSC Testnet
│  Port 8547  │◄──── HTTP RPC  ────► Polygon Amoy
└─────────────┘
```

### Stack tecnológico

| Componente | Tecnología |
|-----------|-----------|
| Consenso | Bullshark DAG (Narwhal + Bullshark BFT) |
| Firma digital | ML-DSA-65 (FIPS 204, post-cuántico) |
| Encriptación | ML-KEM-768 (FIPS 203, threshold mempool) |
| P2P | libp2p 0.53 (QUIC + TCP, Gossipsub, Kademlia) |
| RPC/API | Axum 0.7 + WebSocket tiempo real |
| DEX | AMM Constant Product (x·y=k, 0.3% fee) |
| Bridge | EVM lock/unlock con relayer + BridgeRF.sol |
| Base de datos | Sled (embedded, no-SQL) |
| Ejecución paralela | Rayon (grupos sin conflictos) |
| Frontend | React + Vite + Recharts + lucide-react |
| Monitoring | Prometheus + Grafana |

---

## API Reference

### Estado
```
GET  /status              → Estado del nodo
GET  /network/stats       → Estadísticas completas
GET  /metrics             → Prometheus metrics
GET  /ws                  → WebSocket tiempo real
```

### Cuentas
```
GET  /balance/:addr       → Saldo RF
GET  /account/:addr       → Cuenta completa
GET  /history/:addr       → Historial de TXs
GET  /tokens/:addr        → Saldo de wrapped tokens (wETH, wBNB, wMATIC)
```

### Wallet
```
POST /wallet/new          → Generar nuevo keypair ML-DSA-65
POST /wallet/send         → Enviar TX firmada
POST /wallet/faucet       → Recibir RF del faucet (cooldown 24h)
```

### DEX
```
GET  /dex/pools           → Lista de pools AMM
GET  /dex/pool/:id        → Detalle de pool
GET  /dex/pool/:id/history → Historial de swaps
GET  /dex/pool/:id/prices → Historial de precios
POST /dex/quote           → Cotizar swap
POST /dex/swap            → Ejecutar swap
POST /dex/liquidity/add   → Añadir liquidez
POST /dex/liquidity/remove → Retirar liquidez
GET  /dex/position/:addr/:pool → Posición LP
```

### Bridge
```
GET  /bridge/info         → Estado del bridge
GET  /bridge/chains       → Cadenas EVM soportadas
POST /bridge/mint         → Mintear wrapped token (solo relayer)
```

### Explorer
```
GET  /explorer/search/:q  → Buscar dirección o vértice
GET  /explorer/tx/:hash   → Detalle de TX por hash
```

---

## Flujo completo de usuario

```
1. Abrir https://TU_DOMINIO.COM
   → Crear wallet con frase BIP39 de 12 palabras
   → Contraseña cifra la clave localmente (AES-256-GCM)

2. Obtener RF del faucet
   → 1,000 RF gratis (cooldown 24h por dirección)

3. Bridge EVM → RedFlag
   → Conectar MetaMask en Sepolia/BSC/Polygon
   → Llamar lock() en contrato BridgeRF con ETH/BNB/MATIC
   → El relayer detecta el evento y mintea wETH/wBNB/wMATIC

4. Trading en el DEX
   → Ir a "DEX Trading" en la app
   → Seleccionar pool: RF/wETH, RF/wBNB, RF/wMATIC
   → Hacer swap en tiempo real con precio actualizado
   → Fee: 0.3% (igual a Uniswap V2)

5. Proveer liquidez
   → Add Liquidity → depositar RF + wETH
   → Recibir LP tokens
   → Ganar 0.3% de cada swap proporcional a tu posición

6. Bridge RedFlag → EVM
   → Enviar TX a RedFlag_Bridge_Lock_v1 con datos EVM
   → El relayer llama unlock() en el contrato EVM
   → Recibes ETH/BNB/MATIC en tu wallet EVM
```

---

## Seguridad

- **Post-cuántico**: ML-DSA-65 y ML-KEM-768 resistentes a ataques de computadoras cuánticas
- **Anti-MEV**: Threshold encrypted mempool — las TXs se descifran solo después del commit
- **Rate limiting**: 60 req/min por IP, cooldown 24h en faucet
- **HTTPS**: TLS 1.3 obligatorio en producción
- **Wallet**: Clave privada nunca sale del navegador (cifrada con AES-256-GCM + PBKDF2 200K iter)
- **Bridge**: Nonces únicos previenen replay attacks, daily limit en contratos EVM
- **Docker**: Procesos corren como usuarios no-root

---

## Monitoring

Prometheus scrape en `http://nodo:8545/metrics`:

```
redflag_round             — Ronda de consenso actual
redflag_tx_count          — TXs confirmadas totales
redflag_committed_vertices — Vértices Bullshark confirmados
redflag_pending_txs       — TXs en mempool
redflag_validator_count   — Validadores activos
redflag_fee_pool_balance  — Balance del fee pool
redflag_faucet_balance    — RF restantes en el faucet
redflag_uptime_seconds    — Uptime del nodo
redflag_account_count     — Cuentas con balance
redflag_threshold_round   — Ronda de rotación de llaves ML-KEM
```

Importa `monitoring/grafana_dashboard.json` en Grafana para el dashboard completo.

---

## Variables de entorno

Ver `.env.production.example` para la lista completa.

---

## Licencia

MIT OR Apache-2.0
