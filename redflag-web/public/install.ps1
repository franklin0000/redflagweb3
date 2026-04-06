# redflag.web3 - Node Installer for Windows
# Run with: iwr https://redflagweb3-app.onrender.com/install.ps1 | iex

$ErrorActionPreference = "Stop"
$REPO = "https://github.com/franklin0000/redflagweb3"
$INSTALL_DIR = "$env:USERPROFILE\.redflag\node"
$DATA_DIR = "$env:USERPROFILE\.redflag\data"

Write-Host ""
Write-Host "=============================================" -ForegroundColor Cyan
Write-Host "  redflag.web3 - Node Installer (Windows)  " -ForegroundColor Cyan
Write-Host "  Bullshark DAG - ML-KEM - ML-DSA          " -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan
Write-Host ""

# 1. Instalar Rust si no existe
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "Instalando Rust..." -ForegroundColor Yellow
    $rustup = "$env:TEMP\rustup-init.exe"
    Invoke-WebRequest "https://win.rustup.rs/x86_64" -OutFile $rustup
    & $rustup -y --default-toolchain stable
    $env:PATH += ";$env:USERPROFILE\.cargo\bin"
    Write-Host "OK Rust instalado" -ForegroundColor Green
} else {
    Write-Host "OK Rust encontrado: $(rustc --version)" -ForegroundColor Green
}

# 2. Instalar Git si no existe
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Host "Git no encontrado. Descarga Git desde: https://git-scm.com/download/win" -ForegroundColor Red
    Start-Process "https://git-scm.com/download/win"
    Write-Host "Instala Git y vuelve a ejecutar este script." -ForegroundColor Yellow
    exit 1
}

# 3. Clonar o actualizar repo
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\.redflag" | Out-Null
if (Test-Path "$INSTALL_DIR\.git") {
    Write-Host "Actualizando repositorio..." -ForegroundColor Yellow
    git -C $INSTALL_DIR pull --ff-only
} else {
    Write-Host "Descargando redflag.web3..." -ForegroundColor Yellow
    git clone --depth 1 $REPO $INSTALL_DIR
}

# 4. Compilar
Write-Host "Compilando nodo (5-10 minutos)..." -ForegroundColor Yellow
Set-Location $INSTALL_DIR
cargo build --release -p redflag-network

$BIN = "$INSTALL_DIR\target\release\redflag-network.exe"
if (-not (Test-Path $BIN)) {
    Write-Host "Error: compilacion fallida." -ForegroundColor Red
    exit 1
}
Write-Host "OK Compilado: $BIN" -ForegroundColor Green

# 5. Crear directorio de datos
New-Item -ItemType Directory -Force -Path $DATA_DIR | Out-Null

# 6. Crear script de inicio
$RUNNER = "$env:USERPROFILE\.redflag\start-node.bat"
@"
@echo off
set DATA_DIR=$DATA_DIR
set BOOTSTRAP_PEER=/dns4/redflagweb3-node1.onrender.com/tcp/9000
set P2P_PORT=9000
set PORT=8545
"$BIN"
"@ | Out-File -FilePath $RUNNER -Encoding ASCII

Write-Host ""
Write-Host "=============================================" -ForegroundColor Green
Write-Host "  OK Instalacion completada!               " -ForegroundColor Green
Write-Host "=============================================" -ForegroundColor Green
Write-Host "  Para iniciar el nodo ejecuta:" -ForegroundColor White
Write-Host "  $RUNNER" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Dashboard: http://localhost:8545" -ForegroundColor White
Write-Host "  Chain ID:  2100" -ForegroundColor White
Write-Host "=============================================" -ForegroundColor Green
Write-Host ""

# 7. Preguntar si iniciar ahora
$resp = Read-Host "Iniciar el nodo ahora? (s/n)"
if ($resp -eq "s" -or $resp -eq "S") {
    Write-Host "Iniciando nodo redflag.web3..." -ForegroundColor Yellow
    $env:DATA_DIR = $DATA_DIR
    $env:BOOTSTRAP_PEER = "/dns4/redflagweb3-node1.onrender.com/tcp/9000"
    $env:P2P_PORT = "9000"
    $env:PORT = "8545"
    & $BIN
}
