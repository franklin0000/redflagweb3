#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
#  redflag.web3 — Production Deploy Script
#  Uso: ./deploy.sh TU_DOMINIO.COM [--with-monitoring]
# ═══════════════════════════════════════════════════════════════════
set -e

DOMAIN=${1:-"redflag.example.com"}
WITH_MONITORING=${2:-""}

echo "╔══════════════════════════════════════════════╗"
echo "║   redflag.web3 — Production Deployment        ║"
echo "╚══════════════════════════════════════════════╝"
echo ""
echo "  Dominio: $DOMAIN"
echo ""

# ── 1. Verificar pre-requisitos ───────────────────────────────────────────────
check_command() {
    if ! command -v "$1" &>/dev/null; then
        echo "❌ '$1' no instalado. Instala: $2"
        exit 1
    fi
}

check_command docker "apt install docker.io"
check_command "docker" "docker compose plugin: apt install docker-compose-plugin"
check_command certbot "apt install certbot python3-certbot-nginx"
check_command nginx "apt install nginx"
echo "✅ Pre-requisitos OK"

# ── 2. Crear archivo .env de producción ──────────────────────────────────────
if [ ! -f ".env.production" ]; then
    echo "⚙️  Creando .env.production..."
    cat > .env.production << EOF
# ════ redflag.web3 — Production Environment ════

# Dominio
DOMAIN=$DOMAIN

# Nodos
PORT_NODE1=8545
PORT_NODE2=8546
P2P_PORT_NODE1=9000
P2P_PORT_NODE2=9001

# Bridge
BRIDGE_PORT=8547
BRIDGE_POLL_SECS=12
BRIDGE_CONFIRMATIONS=3
BRIDGE_MAX_AMOUNT=1000000

# EVM RPCs (REEMPLAZA CON TUS KEYS)
ETH_SEPOLIA_RPC=https://eth-sepolia.g.alchemy.com/v2/YOUR_ALCHEMY_KEY
BSC_TESTNET_RPC=https://data-seed-prebsc-1-s1.binance.org:8545
POLYGON_AMOY_RPC=https://polygon-amoy.g.alchemy.com/v2/YOUR_ALCHEMY_KEY

# Bridge contracts (después de deploy)
# ETH_SEPOLIA_BRIDGE_CONTRACT=0x...
# BSC_TESTNET_BRIDGE_CONTRACT=0x...
# POLYGON_AMOY_BRIDGE_CONTRACT=0x...

# Claves del relayer (GENERA NUEVAS)
# BRIDGE_RF_PRIVATE_KEY=hex_de_tu_clave_ml_dsa
# BRIDGE_RELAYER_PRIVATE_KEY=hex_de_tu_clave_eth_privada

# Logs
RUST_LOG=info
EOF
    echo "✅ .env.production creado (edítalo con tus API keys)"
else
    echo "✅ .env.production ya existe"
fi

# ── 3. SSL con Let's Encrypt ─────────────────────────────────────────────────
echo ""
echo "🔒 Configurando SSL para $DOMAIN..."

# Detener nginx si está corriendo
systemctl stop nginx 2>/dev/null || true

# Obtener certificado
if [ ! -d "/etc/letsencrypt/live/$DOMAIN" ]; then
    certbot certonly --standalone -d "$DOMAIN" -d "www.$DOMAIN" \
        -d "rpc2.$DOMAIN" -d "bridge.$DOMAIN" \
        --non-interactive --agree-tos \
        --email "admin@$DOMAIN" \
        --redirect
    echo "✅ Certificado SSL obtenido"
else
    echo "✅ Certificado SSL ya existe"
fi

# ── 4. Configurar nginx ───────────────────────────────────────────────────────
echo ""
echo "🌐 Configurando nginx..."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NGINX_CONF="$SCRIPT_DIR/../nginx/redflag.conf"

# Reemplazar dominio en la config
sed "s/TU_DOMINIO.COM/$DOMAIN/g" "$NGINX_CONF" > /etc/nginx/sites-available/redflag

ln -sf /etc/nginx/sites-available/redflag /etc/nginx/sites-enabled/redflag
rm -f /etc/nginx/sites-enabled/default

nginx -t && echo "✅ Nginx config válida"

# ── 5. Build Docker images ────────────────────────────────────────────────────
echo ""
echo "🐳 Building Docker images..."
cd "$SCRIPT_DIR/../.."
docker compose --env-file .env.production build --no-cache
echo "✅ Images construidas"

# ── 6. Start services ─────────────────────────────────────────────────────────
echo ""
echo "🚀 Iniciando servicios..."
if [ "$WITH_MONITORING" = "--with-monitoring" ]; then
    docker compose --env-file .env.production --profile monitoring up -d
    echo "✅ Nodos + Bridge + Prometheus + Grafana iniciados"
else
    docker compose --env-file .env.production up -d node1 node2 bridge
    echo "✅ Nodos + Bridge iniciados"
fi

# ── 7. Iniciar nginx ──────────────────────────────────────────────────────────
sleep 5
systemctl start nginx
echo "✅ Nginx iniciado"

# ── 8. Auto-renovación SSL ────────────────────────────────────────────────────
if ! crontab -l 2>/dev/null | grep -q "certbot renew"; then
    (crontab -l 2>/dev/null; echo "0 3 * * * certbot renew --quiet --post-hook 'systemctl reload nginx'") | crontab -
    echo "✅ Auto-renovación SSL configurada (cron)"
fi

# ── 9. Verificar estado ───────────────────────────────────────────────────────
echo ""
echo "⏳ Esperando que los nodos arranquen..."
sleep 15

echo ""
echo "═══════════════════════════════════════════════════════"
echo "  ✅ redflag.web3 desplegado en producción"
echo "═══════════════════════════════════════════════════════"
echo ""
echo "  Dashboard:    https://$DOMAIN"
echo "  RPC Node1:    https://$DOMAIN/status"
echo "  RPC Node2:    https://rpc2.$DOMAIN/status"
echo "  Bridge API:   https://bridge.$DOMAIN/bridge/status"
echo "  DEX:          https://$DOMAIN/dex/pools"
echo "  Prometheus:   http://localhost:9090 (si --with-monitoring)"
echo "  Grafana:      http://localhost:3000 (admin/redflag)"
echo ""
echo "  Próximos pasos:"
echo "  1. Edita .env.production con tus API keys de Alchemy/Infura"
echo "  2. Despliega BridgeRF.sol en las testnets EVM"
echo "  3. Agrega BRIDGE_CONTRACT addresses a .env.production"
echo "  4. Reinicia: docker compose restart bridge"
echo ""

# ── Verificación rápida ───────────────────────────────────────────────────────
if curl -sf "https://$DOMAIN/status" > /dev/null 2>&1; then
    echo "  🟢 Node 1: ONLINE"
else
    echo "  🔴 Node 1: verificando... (puede tardar 30s en arrancar)"
fi
