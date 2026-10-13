// Freighter connect button + address + network.
export function ConnectButton({ freighter }) {
  const { connected, address, network, error, connect, disconnect } = freighter;

  if (!connected) {
    return (
      <div>
        <button style={styles.btn} onClick={connect}>
          Connect Freighter
        </button>
        {error && <div style={styles.err}>{error}</div>}
      </div>
    );
  }
  return (
    <div style={styles.row}>
      <span style={styles.addr} title={address}>
        {address.slice(0, 6)}…{address.slice(-6)} ({network})
      </span>
      <button style={styles.ghost} onClick={disconnect}>
        Disconnect
      </button>
    </div>
  );
}

const styles = {
  btn: {
    background: "#3f7cff",
    color: "#fff",
    border: 0,
    borderRadius: 8,
    padding: "10px 18px",
    fontWeight: 700,
    cursor: "pointer",
  },
  ghost: {
    background: "transparent",
    color: "#9fb2cc",
    border: "1px solid #2a2f3a",
    borderRadius: 8,
    padding: "8px 14px",
    cursor: "pointer",
  },
  row: { display: "flex", gap: 12, alignItems: "center" },
  addr: { fontFamily: "monospace", fontSize: 14 },
  err: { color: "#ff8080", fontSize: 13, marginTop: 8 },
};
