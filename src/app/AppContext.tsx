import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, DATA_CHANGED_EVENT, isTauri, type Scope } from "../ipc/client";
import type { ProfileKind } from "../ipc/bindings/ProfileKind";
import type { Settings } from "../ipc/bindings/Settings";
import type { SyncProgress } from "../ipc/bindings/SyncProgress";
import { resolveLanguage } from "../i18n";
import { refreshCachedViews } from "./refreshCachedViews";

interface AppContextValue {
  settings: Settings | undefined;
  syncProgress: SyncProgress | undefined;
  updateSettings: (patch: Partial<Settings>) => void;
  privacy: boolean;
  togglePrivacy: () => void;
  locale: string;
  timeZone: string | null;
  scope: Scope;
  setScope: (scope: Scope) => void;
  profile: ProfileKind | undefined;
  switchProfile: (profile: ProfileKind) => Promise<void>;
}

const AppContext = createContext<AppContextValue | null>(null);

function applyTheme(theme: Settings["theme"]) {
  const root = document.documentElement;
  const resolved =
    theme === "system"
      ? window.matchMedia?.("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark"
      : theme;
  root.dataset.theme = resolved;
}

export function AppProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const { i18n } = useTranslation();
  const [scope, setScope] = useState<Scope>({ kind: "all" });

  const settingsQuery = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const infoQuery = useQuery({ queryKey: ["app-info"], queryFn: api.appInfo });
  const settings = settingsQuery.data;
  const syncQuery = useQuery({
    queryKey: ["sync-progress"],
    queryFn: api.syncProgress,
    enabled: infoQuery.data?.profile === "real",
    refetchInterval: 1000,
  });

  const mutation = useMutation({
    mutationFn: api.updateSettings,
    onSuccess: (next) => {
      queryClient.setQueryData(["settings"], next);
      if (!next.network_console_enabled) queryClient.setQueryData(["network-log"], []);
    },
    onError: () => void queryClient.invalidateQueries({ queryKey: ["settings"] }),
  });

  const updateSettings = useCallback(
    (patch: Partial<Settings>) => {
      if (!settings) return;
      const next = { ...settings, ...patch };
      queryClient.setQueryData(["settings"], next);
      mutation.mutate(next);
    },
    [settings, mutation, queryClient],
  );

  useEffect(() => {
    const theme = settings?.theme ?? "dark";
    applyTheme(theme);
    if (theme !== "system" || !window.matchMedia) return;
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const listener = () => applyTheme("system");
    media.addEventListener("change", listener);
    return () => media.removeEventListener("change", listener);
  }, [settings?.theme]);

  const language = resolveLanguage(settings?.language ?? null);
  useEffect(() => {
    if (i18n.language !== language) void i18n.changeLanguage(language);
    document.documentElement.lang = language;
  }, [language, i18n]);

  // Background synchronization runs in Rust; refetch cached views when it writes.
  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen(DATA_CHANGED_EVENT, () => void refreshCachedViews(queryClient)).then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      }),
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);

  const switchProfile = useCallback(
    async (profile: ProfileKind) => {
      await api.switchProfile(profile);
      setScope({ kind: "all" });
      await queryClient.invalidateQueries();
    },
    [queryClient],
  );

  const value = useMemo<AppContextValue>(
    () => ({
      settings,
      syncProgress: infoQuery.data?.profile === "real" ? syncQuery.data : undefined,
      updateSettings,
      privacy: settings?.privacy_mode ?? false,
      togglePrivacy: () => updateSettings({ privacy_mode: !(settings?.privacy_mode ?? false) }),
      locale: language === "ru" ? "ru-RU" : "en-US",
      timeZone: settings?.timezone ?? null,
      scope,
      setScope,
      profile: infoQuery.data?.profile,
      switchProfile,
    }),
    [
      settings,
      syncQuery.data,
      updateSettings,
      language,
      scope,
      infoQuery.data?.profile,
      switchProfile,
    ],
  );

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used inside AppProvider");
  return ctx;
}
