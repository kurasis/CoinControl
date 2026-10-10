import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
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
  settingsSaveStatus: "saving" | "error" | null;
  retrySettingsSave: () => void;
  privacy: boolean;
  togglePrivacy: () => void;
  locale: string;
  timeZone: string | null;
  scope: Scope;
  setScope: (scope: Scope) => void;
  profile: ProfileKind | undefined;
  switchProfile: (profile: ProfileKind) => Promise<void>;
  viewState: Record<string, unknown>;
  updateViewState: (key: string, update: (previous: unknown) => unknown) => void;
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
  const [viewState, setViewState] = useState<Record<string, unknown>>({});
  const updateViewState = useCallback(
    (key: string, update: (previous: unknown) => unknown) =>
      setViewState((previous) => ({ ...previous, [key]: update(previous[key]) })),
    [],
  );
  const settingsSaves = useRef(new Set<Promise<Settings>>());
  const [failedSettings, setFailedSettings] = useState<Settings | null>(null);

  const settingsQuery = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const infoQuery = useQuery({ queryKey: ["app-info"], queryFn: api.appInfo });
  const settings = settingsQuery.data;
  useEffect(() => {
    if (infoQuery.data?.profile === "real") void api.setSyncScope(scope).catch(() => {});
  }, [scope, infoQuery.data?.profile]);
  const syncQuery = useQuery({
    queryKey: ["sync-progress"],
    queryFn: api.syncProgress,
    enabled: infoQuery.data?.profile === "real",
    refetchInterval: 1000,
  });

  const mutation = useMutation({
    mutationKey: ["settings"],
    scope: { id: "settings" },
    mutationFn: api.updateSettings,
    onMutate: () => setFailedSettings(null),
    onSuccess: (next) => {
      // A queued edit already includes this change. Keep its optimistic state
      // until the final save finishes, rather than flashing an older response.
      if (queryClient.isMutating({ mutationKey: ["settings"] }) > 1) return;
      setFailedSettings(null);
      queryClient.setQueryData(["settings"], next);
      if (!next.network_console_enabled) queryClient.setQueryData(["network-log"], []);
    },
    onError: (_error, next) => {
      if (queryClient.isMutating({ mutationKey: ["settings"] }) === 1) {
        setFailedSettings(next);
        void queryClient.invalidateQueries({ queryKey: ["settings"] });
      }
    },
  });

  const updateSettings = useCallback(
    (patch: Partial<Settings>) => {
      // Read synchronously so multiple edits before React's next render merge.
      const current = queryClient.getQueryData<Settings>(["settings"]);
      if (!current) return;
      const next = { ...current, ...patch };
      queryClient.setQueryData(["settings"], next);
      const save = mutation.mutateAsync(next);
      settingsSaves.current.add(save);
      void save.catch(() => {}).finally(() => settingsSaves.current.delete(save));
    },
    [mutation, queryClient],
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
      // Settings commands address the current store. Drain their queue before
      // changing that store so late saves cannot land in the next profile.
      while (settingsSaves.current.size) await Promise.allSettled([...settingsSaves.current]);
      await api.switchProfile(profile);
      setFailedSettings(null);
      setScope({ kind: "all" });
      setViewState({});
      // Invalidation retains old values while fetching. Profile data must be
      // cleared, including inactive queries and pending reads of the old store.
      await queryClient.resetQueries();
    },
    [queryClient],
  );

  const value = useMemo<AppContextValue>(
    () => ({
      settings,
      syncProgress: infoQuery.data?.profile === "real" ? syncQuery.data : undefined,
      updateSettings,
      settingsSaveStatus: mutation.isPending ? "saving" : failedSettings ? "error" : null,
      retrySettingsSave: () => {
        if (failedSettings && !mutation.isPending) updateSettings(failedSettings);
      },
      // Keep sensitive values masked during startup and profile transitions.
      privacy: settings?.privacy_mode ?? true,
      togglePrivacy: () => updateSettings({ privacy_mode: !(settings?.privacy_mode ?? false) }),
      locale: language === "ru" ? "ru-RU" : "en-US",
      timeZone: settings?.timezone ?? null,
      scope,
      setScope,
      profile: infoQuery.data?.profile,
      switchProfile,
      viewState,
      updateViewState,
    }),
    [
      settings,
      syncQuery.data,
      updateSettings,
      failedSettings,
      mutation.isPending,
      language,
      scope,
      infoQuery.data?.profile,
      switchProfile,
      viewState,
      updateViewState,
    ],
  );

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used inside AppProvider");
  return ctx;
}
