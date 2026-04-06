/**
 * RedFlag Wallet Crypto — AES-256-GCM + PBKDF2
 * Cifrado del lado del cliente usando WebCrypto nativa (sin dependencias externas)
 * La clave privada NUNCA se almacena en texto plano en localStorage.
 */

const PBKDF2_ITERATIONS = 200_000; // NIST SP 800-63B: mínimo 10K, recomendado 100K+
const KEY_LENGTH = 256;
const ALGO = { name: 'AES-GCM', length: KEY_LENGTH };

/** Deriva una clave AES-256 desde una contraseña usando PBKDF2-SHA-256 */
async function deriveKey(password, salt) {
  const enc = new TextEncoder();
  const raw = await crypto.subtle.importKey('raw', enc.encode(password), 'PBKDF2', false, ['deriveKey']);
  return crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: PBKDF2_ITERATIONS, hash: 'SHA-256' },
    raw,
    ALGO,
    false,
    ['encrypt', 'decrypt'],
  );
}

/**
 * Cifra los datos de la wallet con la contraseña del usuario.
 * Devuelve un keystore JSON listo para persistir.
 */
export async function encryptWallet(walletData, password) {
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv   = crypto.getRandomValues(new Uint8Array(12));
  const key  = await deriveKey(password, salt);

  const plaintext = new TextEncoder().encode(JSON.stringify(walletData));
  const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, plaintext);

  return {
    version: 2,
    algo: 'AES-256-GCM',
    kdf: 'PBKDF2-SHA256',
    iterations: PBKDF2_ITERATIONS,
    salt: bufToHex(salt),
    iv: bufToHex(iv),
    cipher: bufToHex(new Uint8Array(ciphertext)),
    created: Date.now(),
  };
}

/**
 * Descifra el keystore con la contraseña.
 * Lanza error si la contraseña es incorrecta (AES-GCM autentica el ciphertext).
 */
export async function decryptWallet(keystore, password) {
  if (keystore.version !== 2) throw new Error('Formato de keystore no soportado');
  const salt = hexToBuf(keystore.salt);
  const iv   = hexToBuf(keystore.iv);
  const data = hexToBuf(keystore.cipher);
  const key  = await deriveKey(password, salt);

  let plaintext;
  try {
    plaintext = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, key, data);
  } catch {
    throw new Error('Contraseña incorrecta o keystore corrupto');
  }
  return JSON.parse(new TextDecoder().decode(plaintext));
}

/** Comprueba si el navegador soporta WebCrypto (siempre true en navegadores modernos) */
export function isCryptoAvailable() {
  return typeof crypto !== 'undefined' && !!crypto.subtle;
}

// ── Utilidades ────────────────────────────────────────────────────────────────

function bufToHex(buf) {
  return Array.from(buf).map(b => b.toString(16).padStart(2, '0')).join('');
}

function hexToBuf(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.slice(i, i + 2), 16);
  }
  return bytes;
}

/** Clave de localStorage donde se guarda el keystore cifrado */
export const KEYSTORE_KEY = 'redflag_keystore_v2';

export function loadKeystore() {
  try { return JSON.parse(localStorage.getItem(KEYSTORE_KEY)); }
  catch { return null; }
}

export function saveKeystore(ks) {
  localStorage.setItem(KEYSTORE_KEY, JSON.stringify(ks));
}

export function deleteKeystore() {
  localStorage.removeItem(KEYSTORE_KEY);
}

export function hasKeystore() {
  return !!localStorage.getItem(KEYSTORE_KEY);
}
