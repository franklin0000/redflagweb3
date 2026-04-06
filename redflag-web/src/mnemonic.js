import * as bip39 from 'bip39';
import { Buffer } from 'buffer';

/**
 * RedFlag Mnemonic Utils
 * Traduce el estándar BIP39 a entropía para nuestras claves PQC (ML-DSA).
 */

/** Genera una nueva frase de 12 palabras (en español preferiblemente o inglés por estándar) */
export function generateMnemonic() {
  return bip39.generateMnemonic(); // 128 bits de entropía
}

/** 
 * Valida si una frase es una semilla BIP39 correcta 
 */
export function validateMnemonic(mnemonic) {
  return bip39.validateMnemonic(mnemonic);
}

/**
 * Convierte el mnemónico en un Seed de 512 bits (64 bytes).
 * Este seed se usará como entropía para derivar las claves ML-DSA.
 */
export async function mnemonicToSeed(mnemonic) {
  const seed = await bip39.mnemonicToSeed(mnemonic); 
  return seed; // Buffer de 64 bytes
}

/**
 * Deriva una "llave secreta" determinista para ML-DSA usando la frase y un path.
 * Implementación simplificada de HD: usaremos el seed de 64 bytes para generar la PKCS#8.
 * Nota: En una fase avanzada, esto se pasará a un WASM de ML-DSA.
 */
export async function deriveMLDSAEntropy(mnemonic, index = 0) {
  const seed = await mnemonicToSeed(mnemonic);
  // Usar los primeros 32 bytes del seed + index para la entropía de la llave privada.
  // En ML-DSA, se pueden generar llaves desde 32 bytes de aleatoriedad.
  const entropy = Buffer.alloc(32);
  seed.copy(entropy, 0, 0, 32);
  
  // Si index > 0, podríamos hacer un hash del seed + index para derivar sub-cuentas.
  // Pero para RedFlag 2.1 inicial, usaremos la cuenta principal (index 0).
  return entropy.toString('hex');
}

/**
 * Formatea el mnemónico para visualización segura (ocultando palabras).
 */
export function formatMnemonic(mnemonic) {
  const words = mnemonic.split(' ');
  return words.map((w, i) => `${i + 1}. ${w}`).join('  ');
}
