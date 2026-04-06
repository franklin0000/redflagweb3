/// Gas Metering — previene loops infinitos y DDoS en contratos
///
/// El gas es la unidad de trabajo computacional.
/// Cada operación WASM tiene un coste. Si se agota, la ejecución revierte.

use crate::gas_costs;

/// Estima el gas de un módulo WASM antes de ejecutarlo
pub fn estimate_gas(bytecode: &[u8], args_len: usize) -> u64 {
    // Estimación base: tamaño del bytecode / 10 instrucciones promedio
    let code_cost = (bytecode.len() as u64 / 10) * gas_costs::WASM_INSTRUCTION;
    let args_cost = args_len as u64 * 4;
    gas_costs::BASE_TX + code_cost + args_cost
}

/// Límite de gas por tipo de operación
pub struct GasLimits;

impl GasLimits {
    /// Gas máximo por transacción de contrato
    pub const MAX_TX_GAS: u64 = 10_000_000;

    /// Gas máximo para deploy
    pub const MAX_DEPLOY_GAS: u64 = 50_000_000;

    /// Gas mínimo requerido para llamar cualquier función
    pub const MIN_CALL_GAS: u64 = 21_000;
}
