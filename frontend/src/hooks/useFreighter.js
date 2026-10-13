// Hook de conexão com a carteira Freighter.
// Baseado no padrão oficial (@stellar/freighter-api).
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

  // Na montagem: detecta se já havia acesso concedido.
  useEffect(() => {
    (async () => {
      try {
        const { isConnected: installed } = await isConnected();
        if (!installed) return;
        const { address: addr } = await getAddress();
        if (!addr) return; // instalado, mas sem acesso concedido ainda
        const { network: net } = await getNetwork();
        setAddress(addr);
        setNetwork(net);
        setConnected(true);
      } catch {
        /* silencioso: usuário ainda não conectou */
      }
    })();
  }, []);

  const connect = useCallback(async () => {
    setError(null);
    try {
      const { isConnected: installed } = await isConnected();
      if (!installed) {
        throw new Error("Extensão Freighter não instalada. Instale e tente de novo.");
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
