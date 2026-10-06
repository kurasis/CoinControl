import { useState } from "react";
import { NavLink, useParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError } from "../ipc/client";
import type { NetworkCapability } from "../ipc/bindings/NetworkCapability";
import type { ProviderStatus } from "../ipc/bindings/ProviderStatus";
import type { Support } from "../ipc/bindings/Support";
import { useApp } from "../app/AppContext";
import { useNetworkNames } from "../app/hooks";
import { Page } from "../components/Layout";
import { AssetPolicyControls } from "../components/AssetPolicyControls";
import { RecoveryControls } from "../components/RecoveryControls";
import { NetworkConsole, NetworkConsoleToggle } from "../components/NetworkConsole";
import { openExternal } from "../lib/external";

export function SettingsPage() {
  const { t } = useTranslation();
  const { tab = "general" } = useParams();
  return (
    <Page title={t("nav.settings")} showScope={false}>
      <nav className="tabs" aria-label={t("settings.sections")}>
        <NavLink to="/settings" end>
          {t("settings.general")}
        </NavLink>
        <NavLink to="/settings/sources">{t("settings.sources")}</NavLink>
        <NavLink to="/settings/networks">{t("settings.networks")}</NavLink>
        <NavLink to="/settings/data">{t("settings.data")}</NavLink>
        <NavLink to="/settings/console">{t("networkLog.title")}</NavLink>
      </nav>
      {tab === "console" ? (
        <NetworkConsole />
      ) : tab === "sources" ? (
        <DataSources />
      ) : tab === "networks" ? (
        <NetworkCoverage />
      ) : tab === "data" ? (
        <DataSettings />
      ) : (
        <GeneralSettings />
      )}
    </Page>
  );
}

function GeneralSettings() {
  const { t } = useTranslation();
  const { settings, updateSettings, privacy, togglePrivacy } = useApp();
  if (!settings) return null;
  const zones =
    typeof Intl.supportedValuesOf === "function" ? Intl.supportedValuesOf("timeZone") : [];
  return (
    <section className="card" aria-label={t("settings.general")}>
      <div className="settings-row">
        <label htmlFor="language">{t("settings.language")}</label>
        <select
          id="language"
          className="select"
          value={settings.language ?? ""}
          onChange={(e) => updateSettings({ language: e.target.value || null })}
        >
          <option value="">{t("settings.followSystem")}</option>
          <option value="en">English</option>
          <option value="ru">Русский</option>
        </select>
      </div>
      <div className="settings-row">
        <span id="theme-label">{t("settings.theme")}</span>
        <div className="segmented" role="group" aria-labelledby="theme-label">
          {(["dark", "light", "system"] as const).map((theme) => (
            <button
              key={theme}
              aria-pressed={settings.theme === theme}
              onClick={() => updateSettings({ theme })}
            >
              {t(`settings.themes.${theme}`)}
            </button>
          ))}
        </div>
      </div>
      <div className="settings-row">
        <label htmlFor="timezone">{t("settings.timezone")}</label>
        <select
          id="timezone"
          className="select"
          value={settings.timezone ?? ""}
          onChange={(e) => updateSettings({ timezone: e.target.value || null })}
        >
          <option value="">
            {t("settings.followSystem")} ({Intl.DateTimeFormat().resolvedOptions().timeZone})
          </option>
          {zones.map((z) => (
            <option key={z} value={z}>
              {z}
            </option>
          ))}
        </select>
      </div>
      <div className="settings-row">
        <label htmlFor="privacy">{t("settings.privacy")}</label>
        <div className="row">
          <input id="privacy" type="checkbox" checked={privacy} onChange={togglePrivacy} />
          <span className="meta">{t("settings.privacyHint")}</span>
        </div>
      </div>
      <div className="settings-row">
        <span>{t("settings.currency")}</span>
        <span className="muted">USD</span>
      </div>
      <div className="settings-row">
        <NavLink to="/settings/console">{t("networkLog.title")}</NavLink>
        <NetworkConsoleToggle />
      </div>
    </section>
  );
}

function storageLabel(p: ProviderStatus, t: (k: string) => string): { text: string; tone: string } {
  if (p.key_requirement === "not_needed") return { text: t("sources.noKeyNeeded"), tone: "" };
  if (p.key_storage === "os_credential_store")
    return { text: t("sources.configured"), tone: "positive" };
  if (p.key_storage === "session_only") return { text: t("sources.sessionOnly"), tone: "warning" };
  return {
    text: t("sources.notConfigured"),
    tone: p.key_requirement === "required" ? "warning" : "",
  };
}

function DataSources() {
  const { t } = useTranslation();
  const providers = useQuery({ queryKey: ["providers"], queryFn: api.listProviders });
  return (
    <>
      <p className="notice meta">{t("sources.intro")}</p>
      <p className="meta">{t("sources.routing")}</p>
      <div className="source-grid">
        {(providers.data ?? []).map((p) => (
          <SourceCard key={p.id} provider={p} />
        ))}
      </div>
    </>
  );
}

function SourceCard({ provider: p }: { provider: ProviderStatus }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const networkNames = useNetworkNames();
  const [key, setKey] = useState("");
  const [reveal, setReveal] = useState(false);
  const save = useMutation({
    mutationFn: () => api.saveProviderKey(p.id, key),
    onSuccess: () => {
      setKey("");
      setReveal(false);
      return Promise.all([
        queryClient.invalidateQueries({ queryKey: ["providers"] }),
        queryClient.invalidateQueries({ queryKey: ["network-capabilities"] }),
      ]);
    },
  });
  const test = useMutation({
    mutationFn: () => api.testProvider(p.id),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["providers"] }),
  });
  const remove = useMutation({
    mutationFn: () => api.removeProviderKey(p.id),
    onSuccess: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: ["providers"] }),
        queryClient.invalidateQueries({ queryKey: ["network-capabilities"] }),
      ]),
  });
  const status = storageLabel(p, t);
  const inputId = `key-${p.id}`;
  const error = save.error ?? remove.error ?? test.error;

  return (
    <article className="card source-card" aria-labelledby={`${p.id}-name`}>
      <header>
        <div className="stack">
          <h3 id={`${p.id}-name`}>{p.name}</h3>
          <span className="meta">{t(`sources.roles.${p.role}`)}</span>
        </div>
        <span className={`chip ${status.tone === "warning" ? "chip-warning" : ""}`}>
          {status.text}
        </span>
      </header>
      <span className="meta">
        {p.networks.length > 0
          ? p.networks.map((n) => networkNames.get(n) ?? n).join(", ")
          : t("sources.allAssets")}
        {" · "}
        {p.free_allowance}
      </span>
      {p.key_requirement !== "not_needed" && (
        <form
          className="stack"
          onSubmit={(e) => {
            e.preventDefault();
            if (key.trim()) save.mutate();
          }}
        >
          <label htmlFor={inputId} className="meta">
            {t(`sources.requirement.${p.key_requirement}`)}
          </label>
          <div className="row">
            <input
              id={inputId}
              className="input"
              type={reveal ? "text" : "password"}
              autoComplete="off"
              spellCheck={false}
              value={key}
              placeholder={
                p.key_storage ? t("sources.replacePlaceholder") : t("sources.keyPlaceholder")
              }
              onChange={(e) => setKey(e.target.value)}
            />
            {key && (
              <button
                type="button"
                className="btn btn-ghost"
                onClick={() => setReveal(!reveal)}
                aria-pressed={reveal}
              >
                {reveal ? t("sources.hide") : t("sources.show")}
              </button>
            )}
          </div>
          <div className="row gap-top">
            <button
              type="submit"
              className="btn btn-primary"
              disabled={!key.trim() || save.isPending}
            >
              {p.key_storage ? t("sources.replace") : t("sources.save")}
            </button>
            {p.key_storage && (
              <button type="button" className="btn" onClick={() => remove.mutate()}>
                {t("sources.remove")}
              </button>
            )}
            <button
              type="button"
              className="btn"
              disabled={!p.adapter_available || test.isPending}
              onClick={() => test.mutate()}
            >
              {t("sources.test")}
            </button>
          </div>
        </form>
      )}
      {p.key_requirement === "not_needed" && (
        <button
          type="button"
          className="btn"
          disabled={!p.adapter_available || test.isPending}
          onClick={() => test.mutate()}
        >
          {t("sources.test")}
        </button>
      )}
      {test.isSuccess && <span className="meta positive">{t("ops.testOk")}</span>}
      {!p.adapter_available && <span className="meta">{t("sources.adapterPending")}</span>}
      {p.last_error && (
        <span className="meta">{t("sources.lastError", { error: p.last_error })}</span>
      )}
      {error && (
        <span className="field-error">
          {isCommandError(error) ? error.message : t("errors.generic")}
        </span>
      )}
      <div className="row meta">
        <span>{t("sources.requestsToday", { count: p.requests_today })}</span>
        {p.estimated_credits_today > 0 && (
          <span>{t("sources.estimatedCreditsToday", { count: p.estimated_credits_today })}</span>
        )}
        <div className="toolbar-spacer" />
        {p.key_url && (
          <button
            type="button"
            className="link-button"
            onClick={() => void openExternal(p.key_url!)}
          >
            {t("sources.getKey")}
          </button>
        )}
        <button type="button" className="link-button" onClick={() => void openExternal(p.docs_url)}>
          {t("sources.docs")}
        </button>
      </div>
    </article>
  );
}

function SupportCell({ level }: { level: Support }) {
  const { t } = useTranslation();
  const tone = level === "full" ? "chip-positive" : level === "partial" ? "chip-warning" : "";
  return <span className={`chip ${tone}`}>{t(`coverage.support.${level}`)}</span>;
}

/** Per-network capability report (SPECIFICATION.md §13, stage D). */
function NetworkCoverage() {
  const { t } = useTranslation();
  const networkNames = useNetworkNames();
  const caps = useQuery({
    queryKey: ["network-capabilities"],
    queryFn: api.listNetworkCapabilities,
    staleTime: Infinity,
  });
  const providers = useQuery({ queryKey: ["providers"], queryFn: api.listProviders });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const status = useQuery({ queryKey: ["sync-status"], queryFn: api.listSyncStatus });
  const providerName = new Map((providers.data ?? []).map((p) => [p.id, p.name]));
  const accountLine = (c: NetworkCapability) => {
    const own = (accounts.data ?? []).filter((a) => a.network === c.network && !a.archived);
    if (own.length === 0) return t("coverage.accountsNone");
    const complete = own.filter(
      (a) => status.data?.find((s) => s.account_id === a.id)?.coverage === "complete",
    ).length;
    return t("coverage.accounts", { count: own.length, complete });
  };
  return (
    <>
      <p className="notice meta">{t("coverage.intro")}</p>
      <div className="card table-scroll">
        <table className="table coverage-table" aria-label={t("settings.networks")}>
          <thead>
            <tr>
              <th>{t("coverage.colNetwork")}</th>
              <th>{t("coverage.colSource")}</th>
              <th>{t("coverage.colBalances")}</th>
              <th>{t("coverage.colTokens")}</th>
              <th>{t("coverage.colHistory")}</th>
              <th>{t("coverage.colFees")}</th>
              <th>{t("coverage.colInternal")}</th>
              <th>{t("coverage.colAccounts")}</th>
            </tr>
          </thead>
          <tbody>
            {(caps.data ?? []).map((c) => (
              <tr key={c.network}>
                <td>
                  <div className="stack">
                    <strong>{networkNames.get(c.network) ?? c.network}</strong>
                    {c.live_verified_on && (
                      <span className="meta">
                        {t("coverage.verified", { date: c.live_verified_on })}
                      </span>
                    )}
                  </div>
                </td>
                <td>
                  <div className="stack">
                    <span>{providerName.get(c.provider) ?? c.provider}</span>
                    <span className="meta">
                      {c.key_required ? t("coverage.keyRequired") : t("coverage.keyNotNeeded")}
                    </span>
                  </div>
                </td>
                <td>
                  <SupportCell level={c.balances} />
                </td>
                <td>
                  <SupportCell level={c.token_discovery} />
                </td>
                <td className="meta">
                  {c.history.map((h) => t(`coverage.history.${h}`)).join(", ")}
                </td>
                <td>
                  <SupportCell level={c.fees} />
                </td>
                <td>
                  <SupportCell level={c.internal_transfers} />
                </td>
                <td className="meta">{accountLine(c)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <section className="card coverage-notes" aria-label={t("coverage.limitations")}>
        <h3>{t("coverage.limitations")}</h3>
        <dl>
          {(caps.data ?? []).map((c) => (
            <div key={c.network} className="coverage-note">
              <dt>{networkNames.get(c.network) ?? c.network}</dt>
              <dd>
                <ul>
                  {c.limitations.map((code) => (
                    <li key={code}>{t(`coverage.limits.${code}`)}</li>
                  ))}
                </ul>
              </dd>
            </div>
          ))}
        </dl>
      </section>
    </>
  );
}

function DataSettings() {
  const { t } = useTranslation();
  const { profile, switchProfile } = useApp();
  const info = useQuery({ queryKey: ["app-info"], queryFn: api.appInfo });
  return (
    <section className="card" aria-label={t("settings.data")}>
      <div className="settings-row">
        <span>{t("settings.dataLocation")}</span>
        <span className="address muted">{info.data?.data_directory ?? "—"}</span>
      </div>
      <div className="settings-row">
        <span>{t("settings.profile")}</span>
        <div className="row">
          <span className="chip">
            {profile === "demo" ? t("settings.profileDemo") : t("settings.profileReal")}
          </span>
          <button
            className="btn"
            onClick={() => void switchProfile(profile === "demo" ? "real" : "demo")}
          >
            {profile === "demo" ? t("demo.exit") : t("welcome.demo")}
          </button>
        </div>
      </div>
      <div className="settings-row">
        <span>{t("settings.versions")}</span>
        <span className="meta">
          {info.data
            ? t("settings.versionLine", {
                version: info.data.version,
                schema: info.data.schema_version,
                engine: info.data.accounting_engine_version,
              })
            : "—"}
        </span>
      </div>
      <div className="settings-row">
        <span>{t("settings.backupExport")}</span>
        <RecoveryControls />
      </div>
      <div className="card-pad">
        <AssetPolicyControls />
      </div>
    </section>
  );
}
