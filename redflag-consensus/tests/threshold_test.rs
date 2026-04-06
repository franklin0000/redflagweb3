use redflag_consensus::threshold::{ThresholdMempool, encrypt_payload};
use redflag_core::{PrivateTxPayload, EncryptedTransaction, CHAIN_ID, MIN_FEE};

#[test]
fn test_threshold_encrypt_decrypt() {
    let tm = ThresholdMempool::new().expect("init");
    let (round, ek) = tm.get_current_ek();

    let payload = PrivateTxPayload {
        receiver: "alice_hex_pubkey".to_string(),
        amount: 1000,
        data: vec![],
        salt: [7u8; 32],
    };

    let (kem_ct, enc, commitment) = encrypt_payload(&ek, &payload, round).expect("encrypt");

    let etx = EncryptedTransaction {
        sender: "bob_hex_pubkey".to_string(),
        nonce: 0,
        chain_id: CHAIN_ID,
        fee: MIN_FEE,
        round,
        payload_commitment: commitment,
        kem_ciphertext: kem_ct,
        encrypted_payload: enc,
        signature: vec![],
    };

    let decrypted = tm.decrypt_payload(&etx).expect("decrypt");
    assert_eq!(decrypted.receiver, payload.receiver);
    assert_eq!(decrypted.amount, 1000);
    println!("✅ Threshold encrypt/decrypt: OK");
}

#[test]
fn test_key_rotation_seals_old_round() {
    let tm = ThresholdMempool::new().expect("init");
    let (old_round, old_ek) = tm.get_current_ek();

    let payload = PrivateTxPayload {
        receiver: "carol".to_string(),
        amount: 50,
        data: vec![],
        salt: [2u8; 32],
    };
    let (kem_ct, enc, commitment) = encrypt_payload(&old_ek, &payload, old_round).expect("encrypt");

    // Rotar a ronda siguiente — sella la llave anterior
    tm.rotate_keys(old_round + 1).expect("rotate");

    let etx = EncryptedTransaction {
        sender: "dave".to_string(),
        nonce: 0,
        chain_id: CHAIN_ID,
        fee: MIN_FEE,
        round: old_round, // ronda pasada
        payload_commitment: commitment,
        kem_ciphertext: kem_ct,
        encrypted_payload: enc,
        signature: vec![],
    };

    // Debe fallar — la DK de la ronda anterior ya fue sellada
    assert!(tm.decrypt_payload(&etx).is_err(), "TX de ronda pasada debe ser rechazada");

    // La llave anterior debe aparecer en el historial revelado
    let revealed = tm.revealed_key_for_round(old_round);
    assert!(revealed.is_some(), "Llave de ronda anterior debe estar en historial");
    println!("✅ Key rotation: ronda sellada y auditada correctamente");
}

#[test]
fn test_multiple_rotations() {
    let tm = ThresholdMempool::new().expect("init");

    for i in 1..=5 {
        tm.rotate_keys(i).expect("rotate");
    }

    let (current_round, ek) = tm.get_current_ek();
    assert_eq!(current_round, 5);
    assert!(!ek.is_empty());

    // Las 4 primeras llaves deben estar en el historial
    for i in 1..=4 {
        assert!(tm.revealed_key_for_round(i).is_some(), "Ronda {} debe estar revelada", i);
    }
    // La ronda 5 aún no está revelada (es la activa)
    assert!(tm.revealed_key_for_round(5).is_none(), "Ronda activa no debe estar revelada");
    println!("✅ 5 rotaciones de llaves: historial correcto");
}
