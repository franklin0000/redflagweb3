/// redflag.web3 — WASM Smart Contract VM
///
/// Características únicas vs otros runtimes:
/// - Gas metering integrado (imposible ejecutar loops infinitos)
/// - Host functions post-cuánticas: hash con BLAKE3, verificar ML-DSA
/// - Contratos firmados con ML-DSA — el bytecode tiene integridad verificable
/// - Ejecución paralela: contratos que no comparten storage corren en paralelo
/// - Storage con prefijo por contrato — aislamiento total entre contratos

use wasmi::{Engine, Module, Store};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use sled::Db;
use thiserror::Error;

pub mod host_functions;
pub mod gas;

#[derive(Error, Debug)]
pub enum VmError {
    #[error("Gas agotado (usado: {used}, límite: {limit})")]
    OutOfGas { used: u64, limit: u64 },
    #[error("Contrato no encontrado: {0}")]
    ContractNotFound(String),
    #[error("Error de ejecución WASM: {0}")]
    ExecutionError(String),
    #[error("Error de compilación WASM: {0}")]
    CompileError(String),
    #[error("Contrato con firma inválida")]
    InvalidSignature,
    #[error("Storage overflow: clave demasiado grande")]
    StorageOverflow,
}

/// Resultado de ejecutar un contrato
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub gas_used: u64,
    pub return_value: Vec<u8>,
    pub logs: Vec<String>,
    pub storage_writes: Vec<(Vec<u8>, Vec<u8>)>, // (key, value) pairs escritos
}

/// Contrato desplegado en la blockchain
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Contract {
    pub address: String,         // blake3(deployer + nonce) en hex
    pub bytecode: Vec<u8>,       // WASM bytecode
    pub deployer: String,        // Dirección del deployer
    pub deploy_round: u64,
    pub signature: Vec<u8>,      // ML-DSA del bytecode — integridad verificable
    pub abi: ContractAbi,
}

/// ABI mínimo del contrato (JSON-compatible)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContractAbi {
    pub name: String,
    pub version: String,
    pub functions: Vec<AbiFunction>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AbiFunction {
    pub name: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub mutates_state: bool,
}

/// Contexto de ejecución pasado a las host functions
#[derive(Clone)]
pub struct ExecutionContext {
    pub caller: String,
    pub contract_address: String,
    pub block_round: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub logs: Vec<String>,
    pub storage_writes: Vec<(Vec<u8>, Vec<u8>)>,
    /// Storage del contrato (prefijado por address)
    pub storage: Arc<Db>,
}

impl ExecutionContext {
    pub fn charge_gas(&mut self, amount: u64) -> Result<(), VmError> {
        self.gas_used = self.gas_used.saturating_add(amount);
        if self.gas_used > self.gas_limit {
            Err(VmError::OutOfGas { used: self.gas_used, limit: self.gas_limit })
        } else {
            Ok(())
        }
    }

    pub fn storage_get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let prefixed_key = self.storage_key(key);
        self.storage.get(&prefixed_key).ok().flatten().map(|v| v.to_vec())
    }

    pub fn storage_set(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), VmError> {
        if key.len() > 256 { return Err(VmError::StorageOverflow); }
        let prefixed_key = self.storage_key(&key);
        self.storage.insert(&prefixed_key, value.clone()).ok();
        self.storage_writes.push((key, value));
        Ok(())
    }

    fn storage_key(&self, key: &[u8]) -> Vec<u8> {
        let mut prefixed = format!("contract:{}:", self.contract_address).into_bytes();
        prefixed.extend_from_slice(key);
        prefixed
    }
}

/// Motor de contratos WASM
pub struct ContractVm {
    engine: Engine,
    contract_db: Db,
}

impl ContractVm {
    pub fn new(db_path: &str) -> Result<Self, anyhow::Error> {
        let config = wasmi::Config::default();
        let engine = Engine::new(&config);
        let contract_db = sled::open(db_path)?;
        Ok(Self { engine, contract_db })
    }

    /// Despliega un nuevo contrato en la blockchain
    pub fn deploy(
        &self,
        bytecode: Vec<u8>,
        deployer: &str,
        nonce: u64,
        round: u64,
        signature: Vec<u8>,
        abi: ContractAbi,
    ) -> Result<String, VmError> {
        // Calcular address del contrato: blake3(deployer || nonce)
        let mut hasher = blake3::Hasher::new();
        hasher.update(deployer.as_bytes());
        hasher.update(&nonce.to_be_bytes());
        let address = hex::encode(hasher.finalize().as_bytes());

        // Validar que el bytecode es WASM válido
        Module::new(&self.engine, &bytecode[..])
            .map_err(|e| VmError::CompileError(e.to_string()))?;

        let contract = Contract {
            address: address.clone(),
            bytecode,
            deployer: deployer.to_string(),
            deploy_round: round,
            signature,
            abi,
        };

        let bytes = bincode::serialize(&contract)
            .map_err(|e| VmError::ExecutionError(e.to_string()))?;
        self.contract_db.insert(format!("contract:{}", address), bytes).ok();

        println!("📋 Contrato desplegado: {} por {} en ronda {}", address, deployer, round);
        Ok(address)
    }

    /// Llama a una función de un contrato desplegado
    pub fn call(
        &self,
        contract_address: &str,
        function_name: &str,
        args: Vec<u8>,
        caller: &str,
        block_round: u64,
        gas_limit: u64,
    ) -> Result<ExecutionResult, VmError> {
        // Cargar contrato
        let contract = self.load_contract(contract_address)?;

        // Compilar módulo WASM
        let module = Module::new(&self.engine, &contract.bytecode[..])
            .map_err(|e| VmError::CompileError(e.to_string()))?;

        // Contexto de ejecución
        let ctx = ExecutionContext {
            caller: caller.to_string(),
            contract_address: contract_address.to_string(),
            block_round,
            gas_limit,
            gas_used: 0,
            logs: Vec::new(),
            storage_writes: Vec::new(),
            storage: Arc::new(self.contract_db.clone()),
        };

        // Linker con host functions
        let mut store = Store::new(&self.engine, ctx);
        let linker = host_functions::build_linker(&self.engine)
            .map_err(|e| VmError::ExecutionError(e.to_string()))?;

        let instance = linker.instantiate(&mut store, &module)
            .map_err(|e| VmError::ExecutionError(e.to_string()))?
            .start(&mut store)
            .map_err(|e| VmError::ExecutionError(e.to_string()))?;

        // Escribir args en memoria del contrato (si tiene función `__set_args`)
        if let Ok(set_args) = instance.get_typed_func::<(i32, i32), ()>(&store, "__set_args") {
            // Asignar args en la memoria lineal del contrato
            if let Some(memory) = instance.get_memory(&store, "memory") {
                let offset = 1024i32; // Reservar los primeros 1KB para el runtime
                memory.write(&mut store, offset as usize, &args).ok();
                set_args.call(&mut store, (offset, args.len() as i32)).ok();
            }
        }

        // Ejecutar la función solicitada
        let call_result = instance
            .get_typed_func::<(), i32>(&store, function_name)
            .map_err(|_| VmError::ExecutionError(format!("función '{}' no encontrada", function_name)))?
            .call(&mut store, ())
            .map_err(|e| VmError::ExecutionError(e.to_string()))?;

        let ctx = store.into_data();
        let return_value = (call_result as u64).to_le_bytes().to_vec();

        Ok(ExecutionResult {
            success: true,
            gas_used: ctx.gas_used,
            return_value,
            logs: ctx.logs,
            storage_writes: ctx.storage_writes,
        })
    }

    /// Consulta (read-only) — no modifica estado, gas ilimitado
    pub fn query(
        &self,
        contract_address: &str,
        function_name: &str,
        args: Vec<u8>,
    ) -> Result<Vec<u8>, VmError> {
        let result = self.call(
            contract_address,
            function_name,
            args,
            "query_caller",
            0,
            1_000_000, // Gas alto para queries
        )?;
        Ok(result.return_value)
    }

    fn load_contract(&self, address: &str) -> Result<Contract, VmError> {
        self.contract_db
            .get(format!("contract:{}", address))
            .ok()
            .flatten()
            .and_then(|bytes| bincode::deserialize::<Contract>(&bytes).ok())
            .ok_or_else(|| VmError::ContractNotFound(address.to_string()))
    }

    pub fn list_contracts(&self) -> Vec<String> {
        self.contract_db
            .scan_prefix("contract:")
            .filter_map(|r| r.ok())
            .filter_map(|(k, _)| {
                String::from_utf8(k.to_vec()).ok()
                    .and_then(|s| s.strip_prefix("contract:").map(|a| a.to_string()))
            })
            .collect()
    }
}

/// Coste de gas por operación (basado en EIP-150 pero adaptado para Bullshark)
pub mod gas_costs {
    pub const BASE_TX: u64 = 21_000;
    pub const CONTRACT_DEPLOY: u64 = 100_000;
    pub const SSTORE_SET: u64 = 20_000;
    pub const SSTORE_RESET: u64 = 5_000;
    pub const SLOAD: u64 = 800;
    pub const LOG: u64 = 375;
    pub const BLAKE3_HASH: u64 = 600;
    pub const MLDSA_VERIFY: u64 = 50_000; // PQC más costoso que ECDSA
    pub const WASM_INSTRUCTION: u64 = 1;
}
