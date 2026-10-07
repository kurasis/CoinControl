import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { api, isCommandError } from "../ipc/client";
import type { Account } from "../ipc/bindings/Account";
import type { Wallet } from "../ipc/bindings/Wallet";
import { useApp } from "../app/AppContext";
import { MASK } from "../lib/format";

export function WalletManagement({ wallet }: { wallet: Wallet }) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [removing, setRemoving] = useState(false);
  const remove = useMutation({
    mutationFn: () => api.removeEmptyWallet(wallet.id),
    onSuccess: () => client.invalidateQueries(),
  });
  const [label, setLabel] = useState(wallet.label);
  const rename = useMutation({
    mutationFn: () => api.renameWallet(wallet.id, label),
    onSuccess: async () => {
      setEditing(false);
      await client.invalidateQueries();
    },
  });
  return (
    <div className="stack">
      {editing ? (
        <form
          className="row"
          onSubmit={(e) => {
            e.preventDefault();
            rename.mutate();
          }}
        >
          <input
            className="input"
            aria-label={t("manage.walletName")}
            value={label}
            maxLength={80}
            onChange={(e) => setLabel(e.target.value)}
          />
          <button className="btn" disabled={!label.trim() || rename.isPending}>
            {t("common.save")}
          </button>
          <button type="button" className="btn" onClick={() => setEditing(false)}>
            {t("common.cancel")}
          </button>
        </form>
      ) : (
        <button
          className="btn btn-ghost"
          onClick={() => {
            setLabel(wallet.label);
            setEditing(true);
          }}
        >
          {t("manage.rename")}
        </button>
      )}
      {wallet.account_count === 0 &&
        (removing ? (
          <div className="row">
            <span>{t("manage.emptyWalletConfirm")}</span>
            <button className="btn" disabled={remove.isPending} onClick={() => remove.mutate()}>
              {t("manage.removePermanently")}
            </button>
            <button className="btn" onClick={() => setRemoving(false)}>
              {t("common.cancel")}
            </button>
          </div>
        ) : (
          <button className="btn btn-ghost" onClick={() => setRemoving(true)}>
            {t("manage.remove")}
          </button>
        ))}
      {remove.isError && (
        <span role="alert">
          {isCommandError(remove.error) ? remove.error.message : t("errors.generic")}
        </span>
      )}
      {rename.isError && (
        <span role="alert">
          {isCommandError(rename.error) ? rename.error.message : t("errors.generic")}
        </span>
      )}
    </div>
  );
}

export function AccountManagement({ account, wallets }: { account: Account; wallets: Wallet[] }) {
  const { t } = useTranslation();
  const { privacy } = useApp();
  const client = useQueryClient();
  const navigate = useNavigate();
  const [destination, setDestination] = useState<string>();
  const [confirmed, setConfirmed] = useState(false);
  const refresh = () => client.invalidateQueries();
  const move = useMutation({
    mutationFn: () => api.moveAccount(account.id, destination ?? account.wallet_id),
    onSuccess: refresh,
  });
  const preview = useMutation({
    mutationFn: () => api.previewAccountRemoval(account.id),
    onMutate: () => setConfirmed(false),
  });
  const remove = useMutation({
    mutationFn: () => api.removeAccount(account.id, preview.data!.revision),
    onSuccess: async () => {
      preview.reset();
      await refresh();
      navigate("/wallets");
    },
  });
  const error = remove.error ?? preview.error ?? move.error;
  return (
    <div className="stack">
      <div className="row">
        <select
          className="select"
          aria-label={t("manage.destination")}
          value={destination ?? account.wallet_id}
          onChange={(e) => setDestination(e.target.value)}
        >
          {wallets.map((w) => (
            <option key={w.id} value={w.id}>
              {w.label}
            </option>
          ))}
        </select>
        <button
          className="btn"
          disabled={
            !destination || destination === account.wallet_id || move.isPending || remove.isPending
          }
          onClick={() => move.mutate()}
        >
          {t("manage.move")}
        </button>
        <button
          className="btn btn-ghost"
          disabled={preview.isPending || remove.isPending}
          onClick={() => preview.mutate()}
        >
          {t("manage.remove")}
        </button>
      </div>
      {preview.data && (
        <section className="notice stack" aria-label={t("manage.removalPreview")}>
          <strong>{t("manage.removalPreview")}</strong>
          <span>
            {account.network} · {privacy ? MASK : account.display_address}
          </span>
          <p>
            {t("manage.removalCounts", {
              movements: preview.data.movements,
              fees: preview.data.fees,
              transactions: preview.data.transactions,
              decisions: preview.data.decisions,
            })}
          </p>
          <p>{t("manage.dependencies")}</p>
          <ul>
            {preview.data.related_accounts.map((a) => (
              <li key={a.id}>
                {wallets.find((w) => w.id === a.wallet_id)?.label} · {a.network} ·{" "}
                {privacy ? MASK : a.display_address}
              </li>
            ))}
          </ul>
          <label className="row-inline">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
            />
            {t("manage.removalConfirm")}
          </label>
          <div className="row">
            <button
              className="btn"
              disabled={!confirmed || remove.isPending}
              onClick={() => remove.mutate()}
            >
              {t("manage.removePermanently")}
            </button>
            <button className="btn" disabled={remove.isPending} onClick={() => preview.reset()}>
              {t("common.cancel")}
            </button>
          </div>
        </section>
      )}
      {error && (
        <span role="alert" className="field-error">
          {isCommandError(error) ? error.message : t("errors.generic")}
        </span>
      )}
    </div>
  );
}
