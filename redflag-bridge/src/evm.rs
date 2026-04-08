use std::sync::Arc;
use std::str::FromStr;
use anyhow::{Context, Result};
use ethers::{
    prelude::*,
    providers::{Provider, Http},
    types::{Address, U256, H256, Filter, Log},
};
use crate::types::{EvmChain, EvmLockEvent, BridgeEventStatus};
use std::time::{SystemTime, UNIX_EPOCH};

/// ABI del contrato bridge en la cadena EVM
/// Eventos que escuchamos:
///   event Locked(address indexed from, string rfAddress, uint256 amount, uint64 nonce)
///   event Unlocked(address indexed to, uint256 amount, uint64 nonce)
abigen!(
    BridgeContract,
    r#"[
        event Locked(address indexed from, string rfAddress, uint256 amount, uint64 nonce)
        event Unlocked(address indexed to, uint256 amount, uint64 nonce)
        function unlock(address to, uint256 amount, uint64 nonce) external
        function lockedBalance() external view returns (uint256)
        function processedNonces(uint64) external view returns (bool)
    ]"#
);

pub struct EvmConnector {
    pub chain:    EvmChain,
    pub provider: Arc<Provider<Http>>,
    pub contract_address: Option<Address>,
    pub wallet:   Option<LocalWallet>,
}

impl EvmConnector {
    /// Crea conector para una cadena EVM. RPC y contrato desde variables de entorno.
    pub fn new(chain: EvmChain) -> Result<Self> {
        let rpc_url = std::env::var(chain.rpc_env_var())
            .with_context(|| format!("Variable {} no configurada", chain.rpc_env_var()))?;

        let provider = Provider::<Http>::try_from(rpc_url.as_str())
            .with_context(|| format!("RPC inválido para {}", chain.name()))?;
        let provider = Arc::new(provider);

        let contract_address = std::env::var(chain.contract_env_var()).ok()
            .and_then(|s| Address::from_str(&s).ok());

        // Clave privada del relayer (para firmar unlock en EVM)
        let wallet = std::env::var("BRIDGE_RELAYER_PRIVATE_KEY").ok()
            .and_then(|pk| {
                pk.parse::<LocalWallet>().ok()
                    .map(|w| w.with_chain_id(chain.chain_id()))
            });

        Ok(Self { chain, provider, contract_address, wallet })
    }

    /// Obtiene el bloque actual
    pub async fn current_block(&self) -> u64 {
        self.provider.get_block_number().await.map(|b| b.as_u64()).unwrap_or(0)
    }

    /// Escanea eventos Locked desde `from_block` hasta `to_block`
    /// Retorna error en lugar de swallowing para que el relayer pueda aplicar backoff
    pub async fn scan_lock_events(&self, from_block: u64, to_block: u64) -> Result<Vec<EvmLockEvent>> {
        let contract_addr = match self.contract_address {
            Some(a) => a,
            None => return Ok(vec![]),
        };

        // topic0 = keccak256("Locked(address,string,uint256,uint64)")
        let locked_topic = H256::from(ethers::utils::keccak256(
            b"Locked(address,string,uint256,uint64)"
        ));

        let filter = Filter::new()
            .address(contract_addr)
            .topic0(locked_topic)
            .from_block(from_block)
            .to_block(to_block);

        let logs: Vec<Log> = self.provider.get_logs(&filter).await
            .with_context(|| format!("get_logs falló en {}", self.chain.name()))?;

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let mut events = Vec::new();

        for log in logs {
            // topics[1] = from address (indexed)
            // data = abi.encode(rfAddress, amount, nonce)
            if log.topics.len() < 2 { continue; }

            let from_addr = format!("0x{}", hex::encode(&log.topics[1].as_bytes()[12..]));

            // Decode non-indexed: string rfAddress, uint256 amount, uint64 nonce
            if let Ok(decoded) = ethers::abi::decode(
                &[
                    ethers::abi::ParamType::String,
                    ethers::abi::ParamType::Uint(256),
                    ethers::abi::ParamType::Uint(64),
                ],
                &log.data,
            ) {
                let rf_address = decoded[0].clone().into_string().unwrap_or_default();
                let amount_wei = decoded[1].clone().into_uint().unwrap_or(U256::zero());
                let nonce = decoded[2].clone().into_uint().unwrap_or(U256::zero()).as_u64();

                // Convertir de wei (18 decimales) a RF units (0 decimales, 1:1 con wei/1e12)
                // 1 RF = 1e12 wei para mantener precisión razonable
                let amount_rf = (amount_wei / U256::from(1_000_000_000_000u64)).as_u64();

                if amount_rf == 0 { continue; }

                let evm_tx_hash = log.transaction_hash
                    .map(|h| format!("{:?}", h))
                    .unwrap_or_default();

                events.push(EvmLockEvent {
                    chain: self.chain.clone(),
                    evm_tx_hash,
                    block_number: log.block_number.map(|b| b.as_u64()).unwrap_or(0),
                    from_evm_address: from_addr,
                    to_rf_address: rf_address,
                    amount: amount_rf,
                    nonce,
                    status: BridgeEventStatus::Pending,
                    created_at: now,
                });
            }
        }

        Ok(events)
    }

    /// Ejecuta `unlock(to, amount, nonce)` en el contrato EVM (liberar tokens del lock)
    pub async fn execute_unlock(&self, to: &str, amount: u64, nonce: u64) -> Result<String> {
        let contract_addr = self.contract_address
            .context("Contrato bridge no configurado")?;
        let wallet = self.wallet.clone()
            .context("Clave del relayer no configurada")?;

        let signer = SignerMiddleware::new(self.provider.clone(), wallet);
        let signer = Arc::new(signer);
        let contract = BridgeContract::new(contract_addr, signer);

        let to_addr = Address::from_str(to)
            .with_context(|| format!("Dirección EVM inválida: {}", to))?;
        let amount_wei = U256::from(amount) * U256::from(1_000_000_000_000u64);

        let tx = contract.unlock(to_addr, amount_wei, nonce).send().await?
            .await?.context("TX unlock fallida")?;

        let hash = format!("{:?}", tx.transaction_hash);
        tracing::info!("✅ Unlock EVM en {}: {} RF → {} (tx: {})",
            self.chain.name(), amount, to, &hash[..16]);
        Ok(hash)
    }

    /// Verifica si un nonce ya fue procesado (evita doble-gasto)
    pub async fn is_nonce_processed(&self, nonce: u64) -> bool {
        let contract_addr = match self.contract_address {
            Some(a) => a,
            None => return false,
        };
        let contract = BridgeContract::new(contract_addr, self.provider.clone());
        contract.processed_nonces(nonce).call().await.unwrap_or(false)
    }

    /// Balance total bloqueado en el contrato
    pub async fn locked_balance(&self) -> u64 {
        let contract_addr = match self.contract_address {
            Some(a) => a,
            None => return 0,
        };
        let contract = BridgeContract::new(contract_addr, self.provider.clone());
        let wei = contract.locked_balance().call().await.unwrap_or(U256::zero());
        (wei / U256::from(1_000_000_000_000u64)).as_u64()
    }
}
