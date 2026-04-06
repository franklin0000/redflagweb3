# Cómo correr un nodo validador en redflag.web3

## Requisitos

- Linux (Ubuntu/Debian/Arch) o macOS
- 2 GB RAM mínimo, 4 GB recomendado
- 20 GB de disco
- Puerto **9000 TCP/UDP abierto** en tu firewall (para P2P)
- IP pública (VPS recomendado: DigitalOcean, Hetzner, Linode)

---

## Instalación rápida (un comando)

```bash
curl -sSf https://redflagweb3-node1.onrender.com/install.sh | bash
```

El script hace todo automáticamente:
1. Instala Rust si no lo tienes
2. Descarga el código fuente desde GitHub
3. Compila el nodo (~5-10 min)
4. Lo configura como servicio systemd
5. Lo conecta al nodo bootstrap principal

---

## Instalación manual

```bash
# 1. Instalar Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Clonar repo
git clone https://github.com/franklin0000/redflagweb3
cd redflagweb3

# 3. Compilar
cargo build --release -p redflag-network

# 4. Ejecutar
DATA_DIR=./node_data \
BOOTSTRAP_PEER=/dns4/redflagweb3-node1.onrender.com/tcp/9000 \
P2P_PORT=9000 \
PORT=8545 \
./target/release/redflag-network
```

---

## Cómo funciona el consenso (Bullshark DAG)

redflag.web3 usa **Bullshark**, un protocolo BFT asíncrono:

1. Cada validador propone **vértices** en rondas de ~5 segundos
2. Los vértices forman un **DAG** (grafo), no una cadena lineal
3. Para confirmar un bloque se necesitan firmas de **2/3 de los validadores**
4. Si un validador cae, la red sigue funcionando (tolera hasta 1/3 de fallos)
5. Todas las comunicaciones usan criptografía post-cuántica **ML-KEM + ML-DSA**

---

## Registro como validador

Una vez que tu nodo esté corriendo, regístrate:

```bash
curl -X POST https://redflagweb3-node1.onrender.com/validators/apply \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Tu nombre o apodo",
    "address": "tu_direccion_RF",
    "description": "Por qué quieres ser validador",
    "multiaddr": "/ip4/TU_IP_PUBLICA/tcp/9000/p2p/TU_PEER_ID"
  }'
```

Tu `address` RF y `PeerID` aparecen cuando inicias el nodo por primera vez.

El owner de la red revisará tu solicitud y añadirá tu clave al conjunto de validadores.

---

## Cómo se obtienen tokens RF

- **Faucet**: `POST /wallet/faucet` — 1,000 RF gratis por dirección cada 24h
- **Bridge**: Bloquea ETH/BNB/MATIC en los contratos y recibes RF equivalente
- **Validar**: Los validadores reciben recompensas de las fees de transacción
- **Transferencia**: Recibe RF de otro usuario de la red

---

## Contratos bridge (mainnet)

| Red | Contrato |
|-----|----------|
| Ethereum | `0xc3Da43E208388c8e24F2339f8D032B7254f3B9d6` |
| BSC | `0xc3Da43E208388c8e24F2339f8D032B7254f3B9d6` |
| Polygon | `0x19D2A913a6df973a7ad600F420960235307c6Cbf` |

---

## Recursos

- Frontend: https://redflagweb3-app.onrender.com
- Nodo RPC: https://redflagweb3-node1.onrender.com
- GitHub: https://github.com/franklin0000/redflagweb3
- Chain ID: **2100**
