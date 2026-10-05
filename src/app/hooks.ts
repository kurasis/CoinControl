import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, type NetworkId } from "../ipc/client";
import { truncateAddress } from "../lib/format";

export function useNetworkNames(): Map<NetworkId, string> {
  const networks = useQuery({
    queryKey: ["networks"],
    queryFn: api.listNetworks,
    staleTime: Infinity,
  });
  return useMemo(() => new Map((networks.data ?? []).map((n) => [n.id, n.name])), [networks.data]);
}

/** "Wallet · 0x1234…abcd" labels for account rows. */
export function useAccountLabels(privacy: boolean): Map<string, string> {
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: api.listWallets });
  return useMemo(() => {
    const walletNames = new Map((wallets.data ?? []).map((w) => [w.id, w.label]));
    return new Map(
      (accounts.data ?? []).map((a) => [
        a.id,
        `${walletNames.get(a.wallet_id) ?? ""} · ${privacy ? "•••••" : truncateAddress(a.display_address)}`,
      ]),
    );
  }, [accounts.data, wallets.data, privacy]);
}
