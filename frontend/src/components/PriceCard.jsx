// Quote card: label, Pyth-formatted price, update time, and state.
import { formatPrice, formatTime } from "../stellar";

export function PriceCard({ label, feedId, data, loading, error }) {
  return (
    <div style={styles.card}>
      <div style={styles.label}>{label}</div>
      <div style={styles.symbol}>Feed #{feedId}</div>
      {loading ? (
        <div style={styles.muted}>Reading from contract…</div>
      ) : error ? (
        <div style={styles.err}>Error: {error}</div>
      ) : data ? (
        <>
          <div style={styles.price}>$ {formatPrice(data.price, data.exponent)}</div>
          <div style={styles.muted}>Updated at {formatTime(data.timestampUs)}</div>
        </>
      ) : (
        <div style={styles.muted}>No quote published yet.</div>
      )}
    </div>
  );
}

const styles = {
  card: {
    border: "1px solid #2a2f3a",
    borderRadius: 12,
    padding: 20,
    minWidth: 240,
    background: "#12151c",
    color: "#e8ecf1",
  },
  label: { fontSize: 13, opacity: 0.7, textTransform: "uppercase" },
  symbol: { fontSize: 20, fontWeight: 700, margin: "4px 0 12px" },
  price: { fontSize: 30, fontWeight: 800 },
  muted: { fontSize: 13, opacity: 0.65, marginTop: 8 },
  err: { fontSize: 13, color: "#ff8080", marginTop: 8 },
};
