// Central config + read-only contract reads via RPC.
//
// The frontend NEVER signs update_price (only the Worker/Admin does).
// Reads use `simulateTransaction` with `Contract.call("get_price", ...)`
// — free, no wallet needed. The wallet (Freighter) identifies the user
// and may sign the user's own calls in the future.

import * as StellarSdk from "@stellar/stellar-sdk";

export const NETWORK = import.meta.env.VITE_STELLAR_NETWORK || "testnet";
export const RPC_URL =
  import.meta.env.VITE_RPC_URL || "https://soroban-testnet.stellar.org";
export const CONTRACT_ID = import.meta.env.VITE_CONTRACT_ID || "";
export const PRICE_SCALE = 10_000_000; // must match the contract

export const server = new StellarSdk.rpc.Server(RPC_URL);
export const networkPassphrase =
  NETWORK === "mainnet" ? StellarSdk.Networks.PUBLIC : StellarSdk.Networks.TESTNET;

/** Reads `get_price(symbol)` from the contract. Returns scaled { price, timestamp } or null. */
export async function fetchPrice(symbol) {
  if (!CONTRACT_ID) throw new Error("VITE_CONTRACT_ID not configured");

  // Throwaway account just for simulation (simulation consumes no real sequence).
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
    // Symbol with no quote yet → contract returns NoPriceData.
    if (JSON.stringify(sim.error).includes("NoPriceData")) return null;
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  // SDK v13 uses `returnValue`; older versions used `retval`.
  const retval = sim.result?.returnValue ?? sim.result?.retval;
  const native = StellarSdk.scValToNative(retval);
  // PriceData arrives as { price: BigInt|string, timestamp: BigInt|string }.
  return {
    price: BigInt(native.price ?? native[0]),
    timestamp: Number(native.timestamp ?? native[1]),
  };
}

/** Converts scaled i128 → "5.1234567" string. */
export function formatScaled(bigintPrice, decimals = 7) {
  const neg = bigintPrice < 0n;
  const abs = neg ? -bigintPrice : bigintPrice;
  const div = 10n ** BigInt(decimals);
  const int = abs / div;
  const frac = (abs % div).toString().padStart(decimals, "0").replace(/0+$/, "") || "0";
  return `${neg ? "-" : ""}${int.toString()}.${frac}`;
}

/** Unix timestamp → local date/time. */
export function formatTime(unixSecs) {
  if (!unixSecs) return "—";
  return new Date(unixSecs * 1000).toLocaleString("en-US");
}
