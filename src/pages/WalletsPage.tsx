import { useEffect, useId, useState, type FormEvent } from "react";
import { Link, useSearchParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError, type NetworkId } from "../ipc/client";
import type { AccountSyncStatus } from "../ipc/bindings/AccountSyncStatus";
import type { NormalizedAddress } from "../ipc/bindings/NormalizedAddress";
import type { Wallet } from "../ipc/bindings/Wallet";
import { useApp } from "../app/AppContext";
import { useNetworkNames } from "../app/hooks";
import { Page } from "../components/Layout";
import { PlusIcon } from "../components/Icons";
import { Spinner, SyncStatus } from "../components/SyncStatus";
import { MASK, formatDateTime } from "../lib/format";

export function WalletsPage() {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const adding = params.get("add") === "1";
  return (
    <Page
      title={t("nav.wallets")}
      showScope={false}
      actions={
        !adding && (
          <>
            <SyncButton />
            <button className="btn btn-primary" onClick={() => setParams({ add: "1" })}>
              <PlusIcon width={16} height={16} />
              {t("addAddress.button")}
            </button>
          </>
        )
      }
    >
      {adding && <AddAddressForm onDone={() => setParams({})} />}
      <SyncStatus />
      <WalletList />
      <GroupsManager />
    </Page>
  );
}

/** Synchronizes every active account now (the real profile only). */
function SyncButton() {
  const { t } = useTranslation();
  const { profile, syncProgress: progress } = useApp();
  const queryClient = useQueryClient();
  const sync = useMutation({
    mutationFn: () => api.syncNow(),
    onSettled: () => queryClient.invalidateQueries(),
  });
  const cancel = useMutation({
    mutationFn: api.cancelSync,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["sync-progress"] });
    },
  });
  if (profile !== "real") return null;
  const running = sync.isPending || progress?.running;
  const failed = sync.data?.accounts.some((a) => a.error !== null) ?? false;
  return (
    <>
      <span className="meta" aria-live="polite">
        {sync.isSuccess && (failed ? t("sync.doneWithErrors") : t("sync.done"))}
        {sync.isError && errorText(sync.error, t)}
      </span>
      <button className="btn" onClick={() => sync.mutate()} disabled={running}>
        {running && <Spinner />}
        {running ? t("sync.running") : t("sync.now")}
      </button>
      {running && (
        <button
          className="btn"
          onClick={() => cancel.mutate()}
          disabled={cancel.isPending || progress?.cancel_requested}
        >
          {t("ops.cancelSync")}
        </button>
      )}
      {progress?.cancel_requested && running && (
        <span role="status">{t("ops.pauseRequested")}</span>
      )}
    </>
  );
}

/** One line describing an account's synchronization state. */
function SyncLine({ status }: { status: AccountSyncStatus | undefined }) {
  const { t } = useTranslation();
  const { locale, timeZone } = useApp();
  if (!status || (status.coverage === null && status.last_error === null)) {
    return <span className="meta">{t("sync.waiting")}</span>;
  }
  if (status.coverage === "unsupported" && status.last_error === null) {
    return <span className="meta">{t("sync.unsupported")}</span>;
  }
  const parts: string[] = [];
  if (status.provider) parts.push(t("sync.source", { provider: status.provider }));
  if (status.balance_only) parts.push(t("sync.balanceReserve"));
  if (status.coverage === "complete") {
    parts.push(t("sync.complete", { count: status.transaction_count }));
  } else if (status.coverage === "partial") {
    parts.push(t("sync.partialHistory", { count: status.transaction_count }));
  } else if (status.coverage === "paused") {
    parts.push(t("sync.pausedHistory", { count: status.transaction_count }));
  } else if (status.coverage !== null) {
    parts.push(t("sync.loading", { count: status.transaction_count }));
  }
  if (status.last_success_at !== null) {
    parts.push(
      t("sync.lastSync", { time: formatDateTime(status.last_success_at, locale, timeZone) }),
    );
  }
  if (status.pending_incomplete) parts.push(t("sync.pendingIncomplete"));
  return (
    <span className="meta">
      {parts.join(" · ")}
      {status.fallback_reasons.length > 0 && (
        <span title={status.fallback_reasons.join("\n")}> {t("sync.reserveUsed")}</span>
      )}
      {status.last_error !== null && (
        <span className="field-error"> {t("sync.failed", { error: status.last_error })}</span>
      )}
    </span>
  );
}

function errorText(e: unknown, t: (k: string) => string): string {
  if (isCommandError(e)) {
    if (e.code === "account_exists") return t("addAddress.duplicate");
    return e.message;
  }
  return t("errors.generic");
}

function AddAddressForm({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const ids = {
    wallet: useId(),
    newWallet: useId(),
    network: useId(),
    address: useId(),
    error: useId(),
  };
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: api.listWallets });
  const networks = useQuery({
    queryKey: ["networks"],
    queryFn: api.listNetworks,
    staleTime: Infinity,
  });
  const [chosenWallet, setWalletId] = useState<string | null>(null);
  const walletId = chosenWallet ?? wallets.data?.[0]?.id ?? "new";
  const [newWallet, setNewWallet] = useState("");
  const [network, setNetwork] = useState<NetworkId>("bitcoin");
  const [address, setAddress] = useState("");
  const [result, setResult] = useState<{
    key: string;
    ok: NormalizedAddress | null;
    error: string | null;
  } | null>(null);
  const addresses = address
    .split(/\r?\n/)
    .map((a) => a.trim())
    .filter(Boolean);
  const trimmed = addresses[0] ?? "";
  const validationKey = `${network}|${trimmed}`;
  // Only show a result for exactly the current input.
  const validation = trimmed && result?.key === validationKey ? result : { ok: null, error: null };

  // Validate locally (in Rust) as the user types; invalid rows are never queried.
  useEffect(() => {
    if (!trimmed) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      api.validateAddress(network, trimmed).then(
        (ok) => !cancelled && setResult({ key: `${network}|${trimmed}`, ok, error: null }),
        (e) =>
          !cancelled &&
          setResult({ key: `${network}|${trimmed}`, ok: null, error: errorText(e, t) }),
      );
    }, 200);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [trimmed, network, t]);

  const submit = useMutation({
    mutationFn: async () => {
      const normalized = await Promise.all(addresses.map((a) => api.validateAddress(network, a)));
      if (
        new Set(normalized.map((a) => a.canonical)).size !== addresses.length ||
        addresses.length > 50
      )
        throw { code: "invalid_input", message: t("ops.batchHint") };
      let target = walletId;
      if (target === "new") target = (await api.createWallet(newWallet)).id;
      return api.addAccounts(target, network, addresses);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries();
      onDone();
    },
  });

  const canSubmit =
    validation.ok !== null &&
    (walletId !== "new" || newWallet.trim().length > 0) &&
    !submit.isPending;

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (canSubmit) submit.mutate();
  }

  return (
    <form className="card card-pad" onSubmit={onSubmit} aria-labelledby="add-heading">
      <div className="section-header">
        <h2 id="add-heading">{t("addAddress.title")}</h2>
      </div>
      <p className="meta">{t("addAddress.publicOnly")}</p>
      <div className="form-grid">
        <div className="field">
          <label htmlFor={ids.wallet}>{t("addAddress.wallet")}</label>
          <select
            id={ids.wallet}
            className="select"
            value={walletId}
            onChange={(e) => setWalletId(e.target.value)}
          >
            {(wallets.data ?? []).map((w) => (
              <option key={w.id} value={w.id}>
                {w.label}
              </option>
            ))}
            <option value="new">{t("addAddress.newWallet")}</option>
          </select>
        </div>
        {walletId === "new" && (
          <div className="field">
            <label htmlFor={ids.newWallet}>{t("addAddress.walletName")}</label>
            <input
              id={ids.newWallet}
              className="input"
              value={newWallet}
              maxLength={80}
              placeholder={t("addAddress.walletNamePlaceholder")}
              onChange={(e) => setNewWallet(e.target.value)}
            />
          </div>
        )}
        <div className="field">
          <label htmlFor={ids.network}>{t("addAddress.network")}</label>
          <select
            id={ids.network}
            className="select"
            value={network}
            onChange={(e) => setNetwork(e.target.value as NetworkId)}
          >
            {(networks.data ?? []).map((n) => (
              <option key={n.id} value={n.id}>
                {n.name}
              </option>
            ))}
          </select>
        </div>
      </div>
      <div className="field gap-top">
        <label htmlFor={ids.address}>{t("addAddress.address")}</label>
        <textarea
          rows={4}
          placeholder={t("ops.batchHint")}
          id={ids.address}
          className="input address"
          value={address}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={validation.error ? true : undefined}
          aria-describedby={ids.error}
          onChange={(e) => setAddress(e.target.value)}
        />
        <div id={ids.error} aria-live="polite">
          {validation.error && <span className="field-error">{validation.error}</span>}
          {validation.ok && (
            <span className="meta">
              {t("addAddress.canonical")} <span className="address">{validation.ok.display}</span>
            </span>
          )}
        </div>
      </div>
      {network === "bitcoin" && <p className="notice meta">{t("addAddress.btcScope")}</p>}
      {network !== "bitcoin" &&
        ["ethereum", "base", "arbitrum", "optimism", "polygon", "bsc"].includes(network) && (
          <p className="notice meta">{t("addAddress.evmScope")}</p>
        )}
      {submit.isError && <p className="field-error">{errorText(submit.error, t)}</p>}
      <div className="row gap-top">
        <button type="submit" className="btn btn-primary" disabled={!canSubmit}>
          {t("addAddress.button")}
        </button>
        <button type="button" className="btn btn-ghost" onClick={onDone}>
          {t("common.cancel")}
        </button>
      </div>
    </form>
  );
}

function WalletList() {
  const { t } = useTranslation();
  const { privacy } = useApp();
  const queryClient = useQueryClient();
  const networkNames = useNetworkNames();
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: api.listWallets });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const syncStatus = useQuery({
    queryKey: ["sync-status"],
    queryFn: api.listSyncStatus,
    // A first import runs in the background; keep the progress line current.
    refetchInterval: (query) =>
      query.state.data?.some((s) => s.coverage === null || s.coverage === "loading")
        ? 5_000
        : false,
  });
  const archive = useMutation({
    mutationFn: ({ id, archived }: { id: string; archived: boolean }) =>
      api.setAccountArchived(id, archived),
    onSuccess: () => queryClient.invalidateQueries(),
  });

  if (wallets.data?.length === 0) {
    return (
      <section className="card empty">
        <h2>{t("wallets.emptyTitle")}</h2>
        <p>{t("wallets.emptyBody")}</p>
      </section>
    );
  }

  return (
    <>
      {(wallets.data ?? []).map((w: Wallet) => (
        <section key={w.id} className="card" aria-label={w.label}>
          <div className="section-header card-pad card-pad-head">
            <h2>{w.label}</h2>
            <span className="meta">{t("wallets.accountCount", { count: w.account_count })}</span>
          </div>
          <div className="list">
            {(accounts.data ?? [])
              .filter((a) => a.wallet_id === w.id)
              .map((a) => (
                <div key={a.id} className="list-row">
                  <span className="chip">{networkNames.get(a.network) ?? a.network}</span>
                  <Link
                    to={`/accounts/${a.id}`}
                    className="address"
                    title={privacy ? undefined : a.display_address}
                  >
                    {privacy ? MASK : a.display_address}
                  </Link>
                  {a.archived && <span className="chip">{t("wallets.archived")}</span>}
                  <SyncLine status={syncStatus.data?.find((s) => s.account_id === a.id)} />
                  <div className="toolbar-spacer" />
                  <button
                    className="btn btn-ghost"
                    onClick={() => void navigator.clipboard?.writeText(a.display_address)}
                    aria-label={t("wallets.copyAddress")}
                  >
                    {t("wallets.copy")}
                  </button>
                  <button
                    className="btn"
                    onClick={() => archive.mutate({ id: a.id, archived: !a.archived })}
                  >
                    {a.archived ? t("wallets.unarchive") : t("wallets.archive")}
                  </button>
                </div>
              ))}
          </div>
        </section>
      ))}
    </>
  );
}

function GroupsManager() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: api.listWallets });
  const groups = useQuery({ queryKey: ["groups"], queryFn: api.listGroups });
  const [label, setLabel] = useState("");
  const create = useMutation({
    mutationFn: () => api.createGroup(label),
    onSuccess: () => {
      setLabel("");
      return queryClient.invalidateQueries({ queryKey: ["groups"] });
    },
  });
  const setMembers = useMutation({
    mutationFn: ({ id, walletIds }: { id: string; walletIds: string[] }) =>
      api.setGroupWallets(id, walletIds),
    onSuccess: () => queryClient.invalidateQueries(),
  });
  const remove = useMutation({
    mutationFn: (id: string) => api.deleteGroup(id),
    onSuccess: () => queryClient.invalidateQueries(),
  });

  return (
    <section id="groups" className="card" aria-labelledby="groups-heading">
      <div className="section-header card-pad card-pad-head">
        <h2 id="groups-heading">{t("nav.groups")}</h2>
        <span className="meta">{t("groups.explain")}</span>
      </div>
      <div className="list">
        {(groups.data ?? []).map((g) => (
          <div key={g.id} className="list-row">
            <Link to={`/groups/${g.id}`}>{g.label}</Link>
            <div className="row">
              {(wallets.data ?? []).map((w) => {
                const checked = g.wallet_ids.includes(w.id);
                return (
                  <label key={w.id} className="chip">
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() =>
                        setMembers.mutate({
                          id: g.id,
                          walletIds: checked
                            ? g.wallet_ids.filter((x) => x !== w.id)
                            : [...g.wallet_ids, w.id],
                        })
                      }
                    />
                    &nbsp;{w.label}
                  </label>
                );
              })}
            </div>
            <div className="toolbar-spacer" />
            <button className="btn btn-ghost" onClick={() => remove.mutate(g.id)}>
              {t("groups.delete")}
            </button>
          </div>
        ))}
        <form
          className="list-row"
          onSubmit={(e) => {
            e.preventDefault();
            if (label.trim()) create.mutate();
          }}
        >
          <input
            className="input"
            value={label}
            maxLength={80}
            placeholder={t("groups.newPlaceholder")}
            aria-label={t("groups.newPlaceholder")}
            onChange={(e) => setLabel(e.target.value)}
          />
          <button type="submit" className="btn" disabled={!label.trim()}>
            {t("groups.create")}
          </button>
        </form>
      </div>
    </section>
  );
}
