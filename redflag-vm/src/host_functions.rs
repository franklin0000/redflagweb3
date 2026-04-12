/// Host Functions — interfaz entre el contrato WASM y la blockchain
///
/// Los contratos pueden llamar estas funciones desde WASM.
/// Cada llamada consume gas según su coste.

use wasmi::{Engine, Linker};
use crate::{ExecutionContext, gas_costs};

pub fn build_linker(engine: &Engine) -> Result<Linker<ExecutionContext>, wasmi::Error> {
    let mut linker = Linker::<ExecutionContext>::new(engine);

    // ── rf_log(ptr: i32, len: i32) ────────────────────────────────────────────
    // Emite un log desde el contrato (visible en el receipt)
    linker.func_wrap("env", "rf_log", |mut caller: wasmi::Caller<ExecutionContext>, ptr: i32, len: i32| {
        let ctx = caller.data_mut();
        let _ = ctx.charge_gas(gas_costs::LOG);
        if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let mut buf = vec![0u8; len as usize];
            if memory.read(&caller, ptr as usize, &mut buf).is_ok() {
                if let Ok(msg) = std::str::from_utf8(&buf) {
                    caller.data_mut().logs.push(msg.to_string());
                }
            }
        }
    })?;

    // ── rf_storage_set(key_ptr, key_len, val_ptr, val_len) ────────────────────
    // Escribe un valor en el storage del contrato
    linker.func_wrap("env", "rf_storage_set", |mut caller: wasmi::Caller<ExecutionContext>, kp: i32, kl: i32, vp: i32, vl: i32| -> i32 {
        let _ = caller.data_mut().charge_gas(gas_costs::SSTORE_SET);
        if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let mut key = vec![0u8; kl as usize];
            let mut val = vec![0u8; vl as usize];
            if memory.read(&caller, kp as usize, &mut key).is_ok()
                && memory.read(&caller, vp as usize, &mut val).is_ok()
            {
                let _ = caller.data_mut().storage_set(key, val);
                return 0; // success
            }
        }
        1 // error
    })?;

    // ── rf_storage_get(key_ptr, key_len, out_ptr) → len ──────────────────────
    // Lee un valor del storage del contrato
    linker.func_wrap("env", "rf_storage_get", |mut caller: wasmi::Caller<ExecutionContext>, kp: i32, kl: i32, out_ptr: i32| -> i32 {
        let _ = caller.data_mut().charge_gas(gas_costs::SLOAD);
        let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
            Some(m) => m,
            None => return -1,
        };
        let mut key = vec![0u8; kl as usize];
        if memory.read(&caller, kp as usize, &mut key).is_err() { return -1; }

        match caller.data().storage_get(&key) {
            Some(val) => {
                let len = val.len() as i32;
                memory.write(&mut caller, out_ptr as usize, &val).ok();
                len
            }
            None => 0,
        }
    })?;

    // ── rf_caller(out_ptr) → len ──────────────────────────────────────────────
    // Devuelve la dirección del llamador
    linker.func_wrap("env", "rf_caller", |mut caller: wasmi::Caller<ExecutionContext>, out_ptr: i32| -> i32 {
        let caller_addr = caller.data().caller.clone();
        let bytes = caller_addr.as_bytes();
        if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            memory.write(&mut caller, out_ptr as usize, bytes).ok();
        }
        bytes.len() as i32
    })?;

    // ── rf_block_round() → u64 ────────────────────────────────────────────────
    // Devuelve la ronda actual del bloque
    linker.func_wrap("env", "rf_block_round", |caller: wasmi::Caller<ExecutionContext>| -> i64 {
        caller.data().block_round as i64
    })?;

    // ── rf_blake3(data_ptr, data_len, out_ptr) ────────────────────────────────
    // Hash BLAKE3 — host function más eficiente que ejecutarlo en WASM
    linker.func_wrap("env", "rf_blake3", |mut caller: wasmi::Caller<ExecutionContext>, dp: i32, dl: i32, out_ptr: i32| {
        let _ = caller.data_mut().charge_gas(gas_costs::BLAKE3_HASH);
        if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let mut data = vec![0u8; dl as usize];
            if memory.read(&caller, dp as usize, &mut data).is_ok() {
                let hash = blake3::hash(&data);
                memory.write(&mut caller, out_ptr as usize, hash.as_bytes()).ok();
            }
        }
    })?;

    // ── rf_gas_remaining() → u64 ─────────────────────────────────────────────
    // Cuánto gas queda disponible
    linker.func_wrap("env", "rf_gas_remaining", |caller: wasmi::Caller<ExecutionContext>| -> i64 {
        let ctx = caller.data();
        ctx.gas_limit.saturating_sub(ctx.gas_used) as i64
    })?;

    // ── rf_balance_of(addr_ptr, addr_len) → i64 ─────────────────────────────
    // Devuelve el balance RF de cualquier dirección (en microRF)
    linker.func_wrap("env", "rf_balance_of", |mut caller: wasmi::Caller<ExecutionContext>, addr_ptr: i32, addr_len: i32| -> i64 {
        let _ = caller.data_mut().charge_gas(200);
        if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let mut buf = vec![0u8; addr_len as usize];
            if memory.read(&caller, addr_ptr as usize, &mut buf).is_ok() {
                if let Ok(addr) = std::str::from_utf8(&buf) {
                    return (caller.data().get_balance)(addr.trim()) as i64;
                }
            }
        }
        -1
    })?;

    // ── rf_transfer(to_ptr, to_len, amount: i64) → i32 ──────────────────────
    // Transfiere RF desde el contrato a una dirección. Devuelve 0=ok, 1=error.
    linker.func_wrap("env", "rf_transfer", |mut caller: wasmi::Caller<ExecutionContext>, to_ptr: i32, to_len: i32, amount: i64| -> i32 {
        let _ = caller.data_mut().charge_gas(5_000);
        if amount <= 0 { return 1; }
        let amount = amount as u64;

        // Verificar que el contrato tiene suficiente balance (descontando transferencias previas)
        let already_sent: u64 = caller.data().pending_transfers.iter().map(|(_, a)| a).sum();
        let available = caller.data().contract_balance.saturating_sub(already_sent);
        if amount > available { return 2; } // insuficiente

        if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let mut buf = vec![0u8; to_len as usize];
            if memory.read(&caller, to_ptr as usize, &mut buf).is_ok() {
                if let Ok(to) = std::str::from_utf8(&buf) {
                    caller.data_mut().pending_transfers.push((to.trim().to_string(), amount));
                    return 0; // ok
                }
            }
        }
        1
    })?;

    // ── rf_emit_event(name_ptr, name_len, data_ptr, data_len) ────────────────
    // Emite un evento nombrado con datos binarios (indexable off-chain)
    linker.func_wrap("env", "rf_emit_event", |mut caller: wasmi::Caller<ExecutionContext>, np: i32, nl: i32, dp: i32, dl: i32| {
        let _ = caller.data_mut().charge_gas(gas_costs::LOG * 2);
        if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let mut name_buf = vec![0u8; nl as usize];
            let mut data_buf = vec![0u8; dl as usize];
            if memory.read(&caller, np as usize, &mut name_buf).is_ok()
                && memory.read(&caller, dp as usize, &mut data_buf).is_ok()
            {
                if let Ok(name) = std::str::from_utf8(&name_buf) {
                    let event = crate::ContractEvent {
                        contract: caller.data().contract_address.clone(),
                        name: name.to_string(),
                        data: data_buf,
                        round: caller.data().block_round,
                    };
                    caller.data_mut().events.push(event);
                }
            }
        }
    })?;

    Ok(linker)
}
