#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
#  redflag.web3 — Bridge Contract Deployment
#  Despliega BridgeRF.sol en múltiples redes EVM
#  Requisito: forge (Foundry) o hardhat
# ═══════════════════════════════════════════════════════════════════
set -e

RELAYER_ADDRESS=${1:-""}
DAILY_LIMIT=${2:-"100000000000000000000"}  # 100 ETH
FEE_BPS=${3:-"10"}  # 0.1%

if [ -z "$RELAYER_ADDRESS" ]; then
    echo "Uso: ./deploy_contracts.sh RELAYER_ETH_ADDRESS [DAILY_LIMIT_WEI] [FEE_BPS]"
    echo "Ejemplo: ./deploy_contracts.sh 0x1234... 100000000000000000000 10"
    exit 1
fi

CONTRACTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/contracts"

echo "╔══════════════════════════════════════════════╗"
echo "║  redflag.web3 Bridge Contract Deployment     ║"
echo "╚══════════════════════════════════════════════╝"
echo ""
echo "  Relayer: $RELAYER_ADDRESS"
echo "  Daily limit: $DAILY_LIMIT wei"
echo "  Fee: $FEE_BPS bps ($(echo "scale=2; $FEE_BPS/100" | bc)%)"
echo ""

# ── Función de deploy ─────────────────────────────────────────────────────────
deploy_to_chain() {
    local CHAIN_NAME=$1
    local RPC_URL=$2
    local CHAIN_ID=$3
    local PRIVATE_KEY=${4:-$DEPLOYER_PRIVATE_KEY}

    if [ -z "$RPC_URL" ]; then
        echo "  ⚠️  $CHAIN_NAME: sin RPC configurado, saltando"
        return
    fi

    echo "  🚀 Desplegando en $CHAIN_NAME (chain_id: $CHAIN_ID)..."

    # Usar Foundry (forge) si está disponible
    if command -v forge &>/dev/null; then
        DEPLOYED=$(forge create \
            --rpc-url "$RPC_URL" \
            --private-key "$PRIVATE_KEY" \
            --broadcast \
            "$CONTRACTS_DIR/BridgeRF.sol:BridgeRF" \
            --constructor-args "$RELAYER_ADDRESS" "$DAILY_LIMIT" "$FEE_BPS" \
            2>&1 | grep "Deployed to:" | awk '{print $3}')

        if [ -n "$DEPLOYED" ]; then
            echo "  ✅ $CHAIN_NAME: $DEPLOYED"
            echo "$CHAIN_NAME=$DEPLOYED" >> deployed_contracts.txt
        else
            echo "  ❌ $CHAIN_NAME: deploy fallido"
        fi
    else
        echo "  ℹ️  Forge no disponible. Instala Foundry: curl -L https://foundry.paradigm.xyz | bash"
        echo "  Luego ejecuta manualmente:"
        echo "    forge create --rpc-url $RPC_URL --private-key \$DEPLOYER_PRIVATE_KEY \\"
        echo "      $CONTRACTS_DIR/BridgeRF.sol:BridgeRF \\"
        echo "      --constructor-args $RELAYER_ADDRESS $DAILY_LIMIT $FEE_BPS"
    fi
}

# ── Deploy en todas las redes ─────────────────────────────────────────────────
echo "Deployed contracts:" > deployed_contracts.txt
echo "Timestamp: $(date)" >> deployed_contracts.txt
echo "" >> deployed_contracts.txt

deploy_to_chain "Ethereum Sepolia" "$ETH_SEPOLIA_RPC"  11155111
deploy_to_chain "BSC Testnet"      "$BSC_TESTNET_RPC"  97
deploy_to_chain "Polygon Amoy"     "$POLYGON_AMOY_RPC" 80002

echo ""
echo "═══════════════════════════════════════════════"
echo "  Contratos desplegados (ver deployed_contracts.txt):"
cat deployed_contracts.txt
echo ""
echo "  Añade estas líneas a .env.production:"
echo "  ETH_SEPOLIA_BRIDGE_CONTRACT=0x..."
echo "  BSC_TESTNET_BRIDGE_CONTRACT=0x..."
echo "  POLYGON_AMOY_BRIDGE_CONTRACT=0x..."
echo ""
echo "  Luego: docker compose restart bridge"
