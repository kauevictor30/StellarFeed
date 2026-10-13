// Freighter wallet connection hook.
// Follows the official pattern (@stellar/freighter-api).
import { useCallback, useEffect, useState } from "react";
import {
  isConnected,
  getAddress,
  getNetwork,
  requestAccess,
} from "@stellar/freighter-api";

export function useFreighter() {
  const [connected, setConnected] = useState(false);
  const [address, setAddress] = useState(null);
  const [network, setNetwork] = useState(null);
  const [error, setError] = useState(null);

  // On mount: detect whether access was already granted.
  useEffect(() => {
    (async () => {
      try {
        const { isConnected: installed } = await isConnected();
        if (!installed) return;
        const { address: addr } = await getAddress();
        if (!addr) return; // installed, but access not granted yet
        const { network: net } = await getNetwork();
        setAddress(addr);
        setNetwork(net);
        setConnected(true);
      } catch {
        /* silent: user hasn't connected yet */
      }
    })();
  }, []);

  const connect = useCallback(async () => {
    setError(null);
    try {
      const { isConnected: installed } = await isConnected();
      if (!installed) {
        throw new Error("Freighter extension not installed. Install it and try again.");
      }
      const { address: addr, error: accessError } = await requestAccess();
      if (accessError) throw new Error(accessError.message);
      const { network: net } = await getNetwork();
      setAddress(addr);
      setNetwork(net);
      setConnected(true);
      return addr;
    } catch (e) {
      setError(e.message);
      throw e;
    }
  }, []);

  const disconnect = useCallback(() => {
    setConnected(false);
    setAddress(null);
    setNetwork(null);
  }, []);

  return { connected, address, network, error, connect, disconnect };
}
