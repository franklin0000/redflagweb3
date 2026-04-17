/**
 * redflag.web3 — Solana Bridge Relayer
 *
 * Vigila transferencias de wRF al bridge wallet.
 * Cuando detecta una, lee el memo (dirección RF destino)
 * y llama a /bridge/mint en redflag.web3.
 *
 * Variables de entorno:
 *   BRIDGE_WALLET_PRIVKEY  — clave privada base58 del bridge wallet
 *   BRIDGE_MINT_SECRET     — secreto compartido con el nodo RF
 *   RF_NODE_URL            — URL del nodo redflag (default: https://redflagweb3-app.onrender.com)
 *   SOLANA_RPC             — RPC de Solana (default: https://solana.publicnode.com)
 *   POLL_INTERVAL_MS       — intervalo de polling (default: 15000)
 */

import { createServer } from 'http';
import { Connection, PublicKey } from '@solana/web3.js';
import { TOKEN_2022_PROGRAM_ID, getAssociatedTokenAddress } from '@solana/spl-token';
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const bs58   = require('bs58');

// ── Health server (required by Render to confirm service is up) ───────────────
const PORT = process.env.PORT || 10000;
createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({ status: 'ok', service: 'redflag-solana-bridge' }));
}).listen(PORT, () => console.log(`Health server listening on port ${PORT}`));

// ── Config ────────────────────────────────────────────────────────────────────

const WRF_MINT       = 'DVqDKrWz8hXgpbjYYNi8sZ69mcQGX6HGDs3dmFk5jZni';
const BRIDGE_WALLET  = process.env.BRIDGE_WALLET || '8b6m2Z8LiqQLpBT9d1q7r8BFJ1EoKUEXvhPz7mWJZUvF';
const BRIDGE_SECRET  = process.env.BRIDGE_MINT_SECRET || '';
const RF_NODE        = process.env.RF_NODE_URL || 'https://redflagweb3-app.onrender.com';
const SOLANA_RPC     = process.env.SOLANA_RPC || 'https://solana.publicnode.com';
const POLL_MS        = parseInt(process.env.POLL_INTERVAL_MS || '15000');

// Ratio de conversión: 1 wRF = 1 RF (misma unidad)
// wRF tiene 6 decimales, RF tiene 6 decimales (microRF)
const WRF_TO_RF_RATE = 1.0;

// ── Estado ────────────────────────────────────────────────────────────────────

const processed = new Set(); // tx signatures ya procesadas
let lastSignature = null;

const connection = new Connection(SOLANA_RPC, { commitment: 'confirmed' });

// ── ATA del bridge wallet ─────────────────────────────────────────────────────

const bridgePubkey = new PublicKey(BRIDGE_WALLET);
const bridgeAta    = await getAssociatedTokenAddress(
  new PublicKey(WRF_MINT),
  bridgePubkey,
  false,
  TOKEN_2022_PROGRAM_ID,
);

console.log('=== redflag.web3 Solana Bridge Relayer ===');
console.log('Bridge wallet:  ', BRIDGE_WALLET);
console.log('Bridge ATA:     ', bridgeAta.toBase58());
console.log('wRF mint:       ', WRF_MINT);
console.log('RF node:        ', RF_NODE);
console.log('Poll interval:  ', POLL_MS, 'ms');
console.log('==========================================\n');

// ── Helpers ───────────────────────────────────────────────────────────────────

function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }

/** Extrae el memo de una transacción (Memo Program v1 y v2) */
function extractMemo(tx) {
  const MEMO_V1 = 'Memo1UhkJRfHyvLMcVucJwxXeuD728EqVDDwQDxFMNo';
  const MEMO_V2 = 'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr';
  if (!tx?.transaction?.message) return null;
  const msg = tx.transaction.message;
  const accounts = msg.accountKeys || msg.staticAccountKeys || [];
  const instructions = msg.instructions || [];
  for (const ix of instructions) {
    const progIdx = ix.programIdIndex;
    const progKey = accounts[progIdx]?.toBase58?.() || accounts[progIdx]?.toString?.() || '';
    if (progKey === MEMO_V1 || progKey === MEMO_V2) {
      // data is base58-encoded memo text
      try {
        const decoded = Buffer.from(bs58.decode(ix.data)).toString('utf8');
        return decoded.trim();
      } catch {
        return ix.data; // already string
      }
    }
  }
  // Try meta logs
  const logs = tx.meta?.logMessages || [];
  for (const log of logs) {
    const m = log.match(/Program log: (.+)/);
    if (m) return m[1].trim();
    const m2 = log.match(/Memo \(len \d+\): "(.+)"/);
    if (m2) return m2[1].trim();
  }
  return null;
}

/** Obtiene el cambio de balance wRF en la ATA del bridge para esta tx */
function getWrfReceived(tx) {
  if (!tx?.meta) return 0n;
  const { preTokenBalances = [], postTokenBalances = [] } = tx.meta;
  const bridgeAtaStr = bridgeAta.toBase58();
  const pre  = preTokenBalances.find(b  => b.mint === WRF_MINT && b.owner === BRIDGE_WALLET);
  const post = postTokenBalances.find(b => b.mint === WRF_MINT && b.owner === BRIDGE_WALLET);
  const preAmt  = BigInt(pre?.uiTokenAmount?.amount  || '0');
  const postAmt = BigInt(post?.uiTokenAmount?.amount || '0');
  return postAmt > preAmt ? postAmt - preAmt : 0n;
}

/** Llama al endpoint /bridge/mint del nodo redflag.web3 */
async function mintRF({ solTxHash, rfAddress, wrfAmount }) {
  // wrfAmount es en micro-wRF (6 decimales)
  // RF también usa 6 decimales → misma cantidad
  const rfAmount = Number(wrfAmount); // micro-RF

  console.log(`  Acreditando ${rfAmount / 1e6} wSOL → ${rfAddress}`);

  const body = {
    evm_tx_hash:   solTxHash,           // usamos la sig de Solana como ID
    to:            rfAddress,
    token:         'wSOL',              // wSOL en redflag.web3
    amount:        rfAmount,
    nonce:         Date.now(),
    bridge_secret: BRIDGE_SECRET,
  };

  try {
    const res = await fetch(`${RF_NODE}/bridge/mint`, {
      method:  'POST',
      headers: { 'Content-Type': 'application/json' },
      body:    JSON.stringify(body),
      signal:  AbortSignal.timeout(15_000),
    });
    const data = await res.json();
    if (res.ok) {
      console.log(`  ✅ RF acreditado! TX RF: ${data.tx_hash || data.hash || 'ok'}`);
      return true;
    } else {
      console.error(`  ❌ Error bridge/mint: ${JSON.stringify(data)}`);
      return false;
    }
  } catch (e) {
    console.error(`  ❌ Error HTTP: ${e.message}`);
    return false;
  }
}

// ── Loop principal ────────────────────────────────────────────────────────────

async function poll() {
  try {
    const opts = { limit: 20, commitment: 'confirmed' };
    if (lastSignature) opts.until = lastSignature;

    const sigs = await connection.getSignaturesForAddress(bridgeAta, opts);
    if (!sigs.length) return;

    // Procesar del más antiguo al más nuevo
    const toProcess = sigs.filter(s => !processed.has(s.signature)).reverse();

    for (const sigInfo of toProcess) {
      const sig = sigInfo.signature;
      if (processed.has(sig)) continue;

      console.log(`\nNueva TX detectada: ${sig.slice(0,20)}…`);

      // Obtener transacción completa
      const tx = await connection.getTransaction(sig, {
        commitment:                       'confirmed',
        maxSupportedTransactionVersion:   0,
      });

      if (!tx) { console.log('  TX no encontrada aún, skip'); continue; }
      if (tx.meta?.err) { console.log('  TX fallida, skip'); processed.add(sig); continue; }

      // Verificar que recibimos wRF
      const received = getWrfReceived(tx);
      if (received === 0n) {
        console.log('  No es transferencia wRF entrante, skip');
        processed.add(sig);
        continue;
      }

      // Extraer dirección RF del memo
      const memo = extractMemo(tx);
      console.log(`  wRF recibidos: ${Number(received) / 1e6}`);
      console.log(`  Memo: ${memo || '(ninguno)'}`);

      if (!memo || memo.length < 16) {
        console.warn('  ⚠️  Sin memo válido — no se puede acreditar RF');
        processed.add(sig);
        continue;
      }

      // Acreditar RF en redflag.web3
      const ok = await mintRF({
        solTxHash:  sig,
        rfAddress:  memo,
        wrfAmount:  received,
      });

      processed.add(sig);
      if (!ok) {
        console.error('  Reintentando en el próximo ciclo…');
        processed.delete(sig); // reintenta
      }
    }

    // Guardar la firma más reciente como cursor
    if (sigs.length > 0) lastSignature = sigs[0].signature;

  } catch (e) {
    console.error('Poll error:', e.message);
  }
}

// ── Arrancar ──────────────────────────────────────────────────────────────────

console.log('Iniciando polling…\n');
while (true) {
  await poll();
  await sleep(POLL_MS);
}
