import { useCallback, useEffect, useState } from "react";
import { useFreighter } from "./hooks/useFreighter";
import { PriceCard } from "./components/PriceCard";
import { ConnectButton } from "./components/ConnectButton";
import { CONTRACT_ID, NETWORK, fetchPrice } from "./stellar";

// Símbolos publicados pelo worker (devem existir no contrato).
const SYMBOLS = [
  { symbol: "BRLXLM", label: "Real / Stellar" },
  { symbol: "BRLUSDC", label: "Real / USD Coin" },
];

const REFRESH_MS = Number(import.meta.env.VITE_REFRESH_MS || 30_000);

export default function App() {
  const freighter = useFreighter();
  const [prices, setPrices] = useState({}); // symbol -> { price, timestamp }
  const [loading, setLoading] = useState(true);
  const [errors, setErrors] = useState({});
  const [lastSync, setLastSync] = useState(null);

  const loadAll = useCallback(async () => {
    setLoading(true);
    const next = {};
    const errs = {};
    await Promise.all(
      SYMBOLS.map(async ({ symbol }) => {
        try {
          next[symbol] = await fetchPrice(symbol);
        } catch (e) {
          errs[symbol] = e.message;
        }
      }),
    );
    setPrices(next);
    setErrors(errs);
    setLoading(false);
    setLastSync(new Date());
  }, []);

  // Carga inicial + polling em tempo real.
  useEffect(() => {
    loadAll();
    const t = setInterval(loadAll, REFRESH_MS);
    return () => clearInterval(t);
  }, [loadAll]);

  return (
    <div style={styles.page}>
      <header style={styles.header}>
        <div>
          <h1 style={styles.title}>✦ StellarFeed</h1>
          <p style={styles.sub}>
            Oráculo de preços nativo da Stellar · {NETWORK} · atualiza a cada{" "}
            {Math.round(REFRESH_MS / 1000)}s
          </p>
          <p style={styles.mono}>contrato: {CONTRACT_ID || "(configure VITE_CONTRACT_ID)"}</p>
        </div>
        <ConnectButton freighter={freighter} />
      </header>

      <main style={styles.grid}>
        {SYMBOLS.map(({ symbol, label }) => (
          <PriceCard
            key={symbol}
            label={label}
            symbol={symbol}
            data={prices[symbol]}
            loading={loading && !prices[symbol]}
            error={errors[symbol]}
          />
        ))}
      </main>

      <footer style={styles.footer}>
        <button style={styles.ghost} onClick={loadAll}>
          ↻ Atualizar agora
        </button>
        {lastSync && (
          <span style={styles.muted}>
            Última sincronização: {lastSync.toLocaleTimeString("pt-BR")}
          </span>
        )}
      </footer>
    </div>
  );
}

const styles = {
  page: {
    maxWidth: 900,
    margin: "0 auto",
    padding: 24,
    fontFamily: "system-ui, sans-serif",
    background: "#0b0e13",
    minHeight: "100vh",
    color: "#e8ecf1",
  },
  header: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "flex-start",
    gap: 16,
    flexWrap: "wrap",
    marginBottom: 24,
  },
  title: { margin: 0, fontSize: 32 },
  sub: { opacity: 0.75, margin: "6px 0" },
  mono: { fontFamily: "monospace", fontSize: 12, opacity: 0.6, wordBreak: "break-all" },
  grid: { display: "flex", gap: 16, flexWrap: "wrap" },
  footer: { marginTop: 24, display: "flex", gap: 12, alignItems: "center" },
  ghost: {
    background: "transparent",
    color: "#9fb2cc",
    border: "1px solid #2a2f3a",
    borderRadius: 8,
    padding: "8px 14px",
    cursor: "pointer",
  },
  muted: { fontSize: 13, opacity: 0.6 },
};
