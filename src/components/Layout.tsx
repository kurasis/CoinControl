import { useLayoutEffect, useRef, type ReactNode } from "react";
import { NavLink, Outlet, useLocation } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../ipc/client";
import { useApp } from "../app/AppContext";
import {
  ActivityIcon,
  EyeIcon,
  EyeOffIcon,
  GroupIcon,
  PortfolioIcon,
  ReviewIcon,
  SettingsIcon,
  WalletIcon,
} from "./Icons";
import { ScopeSelector } from "./ScopeSelector";

export function Layout() {
  const { t } = useTranslation();
  const { profile, switchProfile } = useApp();
  const groups = useQuery({ queryKey: ["groups"], queryFn: api.listGroups });

  return (
    <div className="shell">
      <aside className="sidebar" aria-label={t("nav.label")}>
        <div className="brand">
          <img src="/app-icon.svg" alt="" />
          <span>Portfolio Desk</span>
        </div>
        <nav className="nav" aria-label={t("nav.main")}>
          <NavLink to="/" end aria-label={t("nav.portfolio")} title={t("nav.portfolio")}>
            <PortfolioIcon />
            {t("nav.portfolio")}
          </NavLink>
          <NavLink to="/wallets" aria-label={t("nav.wallets")} title={t("nav.wallets")}>
            <WalletIcon />
            {t("nav.wallets")}
          </NavLink>
          <NavLink to="/activity" aria-label={t("nav.activity")} title={t("nav.activity")}>
            <ActivityIcon />
            {t("nav.activity")}
          </NavLink>
          <NavLink to="/review" aria-label={t("nav.review")} title={t("nav.review")}>
            <ReviewIcon />
            {t("nav.review")}
          </NavLink>
        </nav>
        <nav className="nav" aria-label={t("nav.groups")}>
          <div className="nav-heading">
            <span>{t("nav.groups")}</span>
            <NavLink to="/wallets" className="meta" aria-label={t("groups.manage")}>
              {t("groups.manageShort")}
            </NavLink>
          </div>
          {(groups.data ?? []).map((g) => (
            <NavLink key={g.id} to={`/groups/${g.id}`} aria-label={g.label} title={g.label}>
              <GroupIcon />
              <span className="truncate">{g.label}</span>
            </NavLink>
          ))}
          {groups.data?.length === 0 && <span className="meta nav-note">{t("groups.none")}</span>}
        </nav>
        <div className="sidebar-bottom nav">
          {profile === "demo" && (
            <div className="profile-badge" role="status">
              {t("demo.badge")}{" "}
              <button className="link-button" onClick={() => void switchProfile("real")}>
                {t("demo.exit")}
              </button>
            </div>
          )}
          <NavLink to="/settings" aria-label={t("nav.settings")} title={t("nav.settings")}>
            <SettingsIcon />
            {t("nav.settings")}
          </NavLink>
        </div>
      </aside>
      <main className="main">
        <Outlet />
      </main>
    </div>
  );
}

export function Page({
  title,
  children,
  showScope = true,
  actions,
}: {
  title: string;
  children: ReactNode;
  showScope?: boolean;
  actions?: ReactNode;
}) {
  const { t } = useTranslation();
  const { privacy, togglePrivacy, profile, switchProfile } = useApp();
  const { pathname } = useLocation();
  const toolbarRef = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const main = toolbarRef.current?.closest<HTMLElement>(".main");
    if (main) main.scrollTop = 0;
  }, [pathname]);
  return (
    <>
      <header className="toolbar" ref={toolbarRef}>
        {profile === "demo" && (
          <div className="compact-demo-notice" role="status">
            {t("demo.badge")}{" "}
            <button className="link-button" onClick={() => void switchProfile("real")}>
              {t("demo.exit")}
            </button>
          </div>
        )}
        <h1>{title}</h1>
        {showScope && <ScopeSelector />}
        <div className="toolbar-spacer" />
        {actions}
        <button
          className="btn btn-icon"
          onClick={togglePrivacy}
          aria-pressed={privacy}
          aria-label={privacy ? t("privacy.show") : t("privacy.hide")}
          title={privacy ? t("privacy.show") : t("privacy.hide")}
        >
          {privacy ? <EyeOffIcon /> : <EyeIcon />}
        </button>
      </header>
      <div className="content">{children}</div>
    </>
  );
}
