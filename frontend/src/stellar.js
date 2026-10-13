// Config central + leitura read-only do contrato via RPC.
//
// O frontend NUNCA assina update_price (só o Worker/Admin).
// Leitura usa `simulateTransaction` com `Contract.call("get_price", ...)`
// — gratuita, sem carteira necessária. A carteira (Freighter) serve para
// identificar o usuário e, no futuro, assinar chamadas próprias.

import * as StellarSdk from "@stellar/stellar-sdk";

export const NETWORK = import.meta.env.VITE_STELLAR_NETWORK || "testnet";
export const RPC_URL =
  import.meta.env.VITE_RPC_URL || "https://soroban-testnet.stellar.org";
export const CONTRACT_ID = import.meta.env.VITE_CONTRACT_ID || "";
export const PRICE_SCALE = 10_000_000; // deve bater com o contrato

export const server = new StellarSdk.rpc.Server(RPC_URL);
export const networkPassphrase =
  NETWORK === "mainnet" ? StellarSdk.Networks.PUBLIC : StellarSdk.Networks.TESTNET;

/** Lê `get_price(symbol)` do contrato. Retorna { price, timestamp } escalado ou null. */
export async function fetchPrice(symbol) {
  if (!CONTRACT_ID) throw new Error("VITE_CONTRACT_ID não configurado");

  // Conta efêmera só para simular (simulação não consome sequência real).
  const source = StellarSdk.Keypair.random();
  const account = await server.getAccount(source.publicKey());

  const contract = new StellarSdk.Contract(CONTRACT_ID);
  const op = contract.call(
    "get_price",
    StellarSdk.nativeToScVal(symbol, { type: "symbol" }),
  );

  const tx = new StellarSdk.TransactionBuilder(account, {
    fee: StellarSdk.BASE_FEE,
    networkPassphrase,
  })
    .addOperation(op)
    .setTimeout(30)
    .build();

  const sim = await server.simulateTransaction(tx);
  if (StellarSdk.rpc.Api.isSimulationError(sim)) {
    // Símbolo sem cotação ainda → contrato retorna NoPriceData.
    if (JSON.stringify(sim.error).includes("NoPriceData")) return null;
    throw new Error(`Simulação falhou: ${sim.error}`);
  }
  // SDK v13 usa `returnValue`; versões antigas usavam `retval`.
  const retval = sim.result?.returnValue ?? sim.result?.retval;
  const native = StellarSdk.scValToNative(retval);
  // PriceData chega como { price: BigInt|string, timestamp: BigInt|string }.
  return {
    price: BigInt(native.price ?? native[0]),
    timestamp: Number(native.timestamp ?? native[1]),
  };
}

/** Converte i128 escalado → string "5,1234567". */
export function formatScaled(bigintPrice, decimals = 7) {
  const neg = bigintPrice < 0n;
  const abs = neg ? -bigintPrice : bigintPrice;
  const div = 10n ** BigInt(decimals);
  const int = abs / div;
  const frac = (abs % div).toString().padStart(decimals, "0").replace(/0+$/, "") || "0";
  return `${neg ? "-" : ""}${int.toString()},${frac}`;
}

/** Timestamp unix → data/hora local. */
export function formatTime(unixSecs) {
  if (!unixSecs) return "—";
  return new Date(unixSecs * 1000).toLocaleString("pt-BR");
}
