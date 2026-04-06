#!/usr/bin/env bash
# redflag.web3 — Node Installer
# Usage: curl -sSf https://redflagweb3-node1.onrender.com/install.sh | bash

set -e

REPO="https://github.com/franklin0000/redflagweb3"
BOOTSTRAP="/dns4/redflagweb3-node1.onrender.com/tcp/9000"
NODE_BIN="redflag-network"
DATA_DIR="$HOME/.redflag/data"
SERVICE_NAME="redflag-node"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'

echo ""
echo "╔══════════════════════════════════════════════╗"
echo "║     redflag.web3 — Node Installer v1.0       ║"
echo "║     Bullshark DAG • ML-KEM • ML-DSA          ║"
echo "╚══════════════════════════════════════════════╝"
echo ""

# ── 1. Detectar OS ──────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"
echo -e "${YELLOW}Sistema: $OS ($ARCH)${NC}"

if [[ "$OS" != "Linux" && "$OS" != "Darwin" ]]; then
  echo -e "${RED}Error: Solo Linux y macOS están soportados.${NC}"
  exit 1
fi

# ── 2. Verificar / instalar Rust ────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
  echo -e "${YELLOW}Instalando Rust...${NC}"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  source "$HOME/.cargo/env"
else
  echo -e "${GREEN}✅ Rust $(rustc --version | cut -d' ' -f2) encontrado${NC}"
fi

# ── 3. Dependencias del sistema ─────────────────────────────────────────────
if [[ "$OS" == "Linux" ]]; then
  if command -v apt-get &>/dev/null; then
    sudo apt-get install -y build-essential pkg-config libssl-dev git 2>/dev/null || true
  elif command -v dnf &>/dev/null; then
    sudo dnf install -y gcc openssl-devel git 2>/dev/null || true
  fi
fi

# ── 4. Clonar repo ──────────────────────────────────────────────────────────
INSTALL_DIR="$HOME/.redflag/node"
if [[ -d "$INSTALL_DIR/.git" ]]; then
  echo -e "${YELLOW}Actualizando repositorio...${NC}"
  git -C "$INSTALL_DIR" pull --ff-only
else
  echo -e "${YELLOW}Descargando redflag.web3...${NC}"
  git clone --depth 1 "$REPO" "$INSTALL_DIR"
fi

# ── 5. Compilar ─────────────────────────────────────────────────────────────
echo -e "${YELLOW}Compilando nodo (esto toma ~5-10 minutos)...${NC}"
cd "$INSTALL_DIR"
cargo build --release -p redflag-network 2>&1 | grep -E "Compiling|Finished|error" || true

BIN="$INSTALL_DIR/target/release/$NODE_BIN"
if [[ ! -f "$BIN" ]]; then
  echo -e "${RED}Error: Compilación fallida. Revisa los errores arriba.${NC}"
  exit 1
fi
echo -e "${GREEN}✅ Compilado: $BIN${NC}"

# ── 6. Configurar directorio de datos ───────────────────────────────────────
mkdir -p "$DATA_DIR"

# ── 7. Crear script de inicio ───────────────────────────────────────────────
RUNNER="$HOME/.redflag/start-node.sh"
cat > "$RUNNER" <<EOF
#!/usr/bin/env bash
export DATA_DIR="$DATA_DIR"
export BOOTSTRAP_PEER="$BOOTSTRAP"
export P2P_PORT=9000
export PORT=8545
exec "$BIN"
EOF
chmod +x "$RUNNER"

# ── 8. Crear servicio systemd (Linux) ───────────────────────────────────────
if [[ "$OS" == "Linux" ]] && command -v systemctl &>/dev/null; then
  SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
  sudo tee "$SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=redflag.web3 Blockchain Node
After=network.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$INSTALL_DIR
ExecStart=$RUNNER
Restart=always
RestartSec=10
Environment=DATA_DIR=$DATA_DIR
Environment=BOOTSTRAP_PEER=$BOOTSTRAP
Environment=P2P_PORT=9000
Environment=PORT=8545

[Install]
WantedBy=multi-user.target
EOF
  sudo systemctl daemon-reload
  sudo systemctl enable "$SERVICE_NAME"
  sudo systemctl start "$SERVICE_NAME"
  echo -e "${GREEN}✅ Servicio systemd iniciado: $SERVICE_NAME${NC}"
  echo ""
  echo "  Comandos útiles:"
  echo "  systemctl status $SERVICE_NAME"
  echo "  journalctl -fu $SERVICE_NAME"
else
  echo -e "${GREEN}✅ Para iniciar el nodo ejecuta:${NC}"
  echo "  $RUNNER"
fi

# ── 9. Obtener PeerID y dirección RF ────────────────────────────────────────
echo ""
echo -e "${YELLOW}Iniciando nodo brevemente para obtener tu PeerID...${NC}"
DATA_DIR="$DATA_DIR" PORT=8545 P2P_PORT=9000 timeout 5 "$BIN" 2>/dev/null | grep -E "PeerID|Faucet|📍" || true

echo ""
echo "╔══════════════════════════════════════════════╗"
echo "║  ✅ Instalación completada                   ║"
echo "╠══════════════════════════════════════════════╣"
echo "║  Tu nodo está corriendo en :8545             ║"
echo "║  Red: redflag.web3  Chain ID: 2100           ║"
echo "╠══════════════════════════════════════════════╣"
echo "║  Para ser validador registrate en:           ║"
echo "║  https://redflagweb3-node1.onrender.com      ║"
echo "║  POST /validators/apply                      ║"
echo "╚══════════════════════════════════════════════╝"
echo ""
