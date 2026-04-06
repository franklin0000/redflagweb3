use serde::{Serialize, Deserialize};

/// Cadenas EVM soportadas
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EvmChain {
    EthereumSepolia,
    BscTestnet,
    PolygonAmoy,
    // Agregar más cadenas aquí
}

impl EvmChain {
    pub fn chain_id(&self) -> u64 {
        match self {
            Self::EthereumSepolia => 11155111,
            Self::BscTestnet      => 97,
            Self::PolygonAmoy     => 80002,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::EthereumSepolia => "Ethereum Sepolia",
            Self::BscTestnet      => "BSC Testnet",
            Self::PolygonAmoy     => "Polygon Amoy",
        }
    }

    pub fn rpc_env_var(&self) -> &'static str {
        match self {
            Self::EthereumSepolia => "ETH_SEPOLIA_RPC",
            Self::BscTestnet      => "BSC_TESTNET_RPC",
            Self::PolygonAmoy     => "POLYGON_AMOY_RPC",
        }
    }

    pub fn contract_env_var(&self) -> &'static str {
        match self {
            Self::EthereumSepolia => "ETH_SEPOLIA_BRIDGE_CONTRACT",
            Self::BscTestnet      => "BSC_TESTNET_BRIDGE_CONTRACT",
            Self::PolygonAmoy     => "POLYGON_AMOY_BRIDGE_CONTRACT",
        }
    }

    pub fn all() -> Vec<EvmChain> {
        vec![Self::EthereumSepolia, Self::BscTestnet, Self::PolygonAmoy]
    }
}

/// Estado de un evento de puente
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BridgeEventStatus {
    Pending,
    Processing,
    Completed,
    Failed(String),
}

/// Dirección especial en RedFlag para operaciones de puente
pub const BRIDGE_LOCK_ADDRESS:  &str = "RedFlag_Bridge_Lock_v1";
pub const BRIDGE_BURN_ADDRESS:  &str = "RedFlag_Bridge_Burn_v1";

/// Evento de bloqueo en la cadena EVM → mintear en RedFlag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmLockEvent {
    pub chain:              EvmChain,
    pub evm_tx_hash:        String,
    pub block_number:       u64,
    pub from_evm_address:   String,
    pub to_rf_address:      String,
    pub amount:             u64,   // en RF units (18 decimales → convertido)
    pub nonce:              u64,
    pub status:             BridgeEventStatus,
    pub created_at:         u64,
}

/// Evento de bloqueo en RedFlag → mintear/liberar en EVM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfLockEvent {
    pub rf_tx_hash:         String,
    pub from_rf_address:    String,
    pub to_evm_address:     String,
    pub to_chain:           EvmChain,
    pub amount:             u64,
    pub nonce:              u64,
    pub status:             BridgeEventStatus,
    pub created_at:         u64,
}

/// Resumen del estado del bridge para la API
#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeSummary {
    pub chains:                 Vec<ChainInfo>,
    pub total_locked_rf:        u64,
    pub total_evm_lock_events:  usize,
    pub total_rf_lock_events:   usize,
    pub pending_events:         usize,
    pub completed_events:       usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChainInfo {
    pub name:           String,
    pub chain_id:       u64,
    pub connected:      bool,
    pub contract:       Option<String>,
    pub last_block:     u64,
}
