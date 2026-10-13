// Central config + read-only contract reads via RPC.
//
// The frontend NEVER submits prices (relaying needs no trust, but fees
// make spam pointless; reads are free). Reads use `simulateTransaction`
// with `Contract.call("get_price", ...)` — free, no wallet needed.
// The wallet (Freighter) identifies the user.

import * as StellarSdk from "@stellar/stellar-sdk";

export const NETWORK = import.meta.env.VITE_STELLAR_NETWORK || "testnet";
export const RPC_URL =
  import.meta.env.VITE_RPC_URL || "https://soroban-testnet.stellar.org";
export const CONTRACT_ID = import.meta.env.VITE_CONTRACT_ID || "";

// "feedId:Label" list, e.g. "1:BTC/USD,2:ETH/USD". Feed IDs come from
// https://docs.pyth.network/price-feeds/pro/price-feed-ids
export const FEEDS = (import.meta.env.VITE_FEEDS || "1:BTC/USD,2:ETH/USD")
  .split(",")
  .map((entry) => {
    const [id, ...label] = entry.split(":");
    return { feedId: Number(id), label: label.join(":") || `Feed ${id}` };
  });

export const server = new StellarSdk.rpc.Server(RPC_URL);
export const networkPassphrase =
  NETWORK === "mainnet" ? StellarSdk.Networks.PUBLIC : StellarSdk.Networks.TESTNET;

/** Reads `get_price(feed_id)` from the contract. Returns { price, exponent, timestampUs } or null. */
export async function fetchPrice(feedId) {
  if (!CONTRACT_ID) throw new Error("VITE_CONTRACT_ID not configured");

  // Throwaway account just for simulation (simulation consumes no real sequence).
  const source = StellarSdk.Keypair.random();
  const account = await server.getAccount(source.publicKey());

  const contract = new StellarSdk.Contract(CONTRACT_ID);
  const op = contract.call(
    "get_price",
    StellarSdk.nativeToScVal(feedId, { type: "u32" }),
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
    // Feed with no quote yet → contract returns NoPriceData.
    if (JSON.stringify(sim.error).includes("NoPriceData")) return null;
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  // SDK v13 uses `returnValue`; older versions used `retval`.
  const retval = sim.result?.returnValue ?? sim.result?.retval;
  const native = StellarSdk.scValToNative(retval);
  // StoredPrice arrives as { price: BigInt, exponent: number, timestamp_us: BigInt }.
  return {
    price: BigInt(native.price),
    exponent: Number(native.exponent),
    timestampUs: BigInt(native.timestamp_us ?? native.timestampUs),
  };
}

/** Converts Pyth (price, exponent) → human string. Real value = price * 10^exponent. */
export function formatPrice(bigintPrice, exponent) {
  const neg = bigintPrice < 0n;
  let digits = (neg ? -bigintPrice : bigintPrice).toString();
  if (exponent >= 0) return `${neg ? "-" : ""}${digits}${"0".repeat(exponent)}`;
  const point = digits.length + exponent; // exponent is negative
  if (point > 0) {
    const int = digits.slice(0, point);
    const frac = digits.slice(point).replace(/0+$/, "") || "0";
    return `${neg ? "-" : ""}${int}.${frac}`;
  }
  const frac = `${"0".repeat(-point)}${digits}`.replace(/0+$/, "") || "0";
  return `${neg ? "-" : ""}0.${frac}`;
}

/** Microseconds timestamp → local date/time. */
export function formatTime(timestampUs) {
  if (!timestampUs) return "—";
  return new Date(Number(timestampUs / 1000n)).toLocaleString("en-US");
}
