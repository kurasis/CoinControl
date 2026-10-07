import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { api } from "../ipc/client";
import type { Settings } from "../ipc/bindings/Settings";
import "../i18n";
import { AppProvider, useApp } from "./AppContext";

vi.mock("../ipc/client", () => ({
  DATA_CHANGED_EVENT: "portfolio-data-changed",
  isTauri: () => false,
  api: {
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
    appInfo: vi.fn(),
    switchProfile: vi.fn(),
    syncProgress: vi.fn().mockResolvedValue(undefined),
  },
}));

const initial: Settings = {
  language: "en",
  theme: "dark",
  timezone: null,
  privacy_mode: false,
  network_console_enabled: false,
  price_refresh_seconds: 60,
  sweep_interval_minutes: 60,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

let app: ReturnType<typeof useApp>;
let client: QueryClient;

function Probe({ read }: { read?: () => Promise<string> }) {
  const context = useApp();
  useEffect(() => {
    app = context;
  }, [context]);
  const value = useQuery({
    queryKey: ["profile-value"],
    queryFn: read ?? (() => Promise.resolve("")),
    enabled: !!read,
  });
  return <output>{value.data ?? "loading"}</output>;
}

async function mount(read?: () => Promise<string>) {
  client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  render(
    <QueryClientProvider client={client}>
      <AppProvider>
        <Probe read={read} />
      </AppProvider>
    </QueryClientProvider>,
  );
  await waitFor(() => expect(app.settings).toEqual(initial));
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getSettings).mockResolvedValue(initial);
  vi.mocked(api.appInfo).mockResolvedValue({
    product_name: "Portfolio Desk",
    version: "test",
    profile: "demo",
    schema_version: 7,
    accounting_engine_version: 3,
    data_directory: "test",
  });
});

afterEach(() => {
  cleanup();
  client?.clear();
});

it("merges rapid setting edits and serializes saves without flashing an older response", async () => {
  const first = deferred<Settings>();
  const second = deferred<Settings>();
  vi.mocked(api.updateSettings)
    .mockImplementationOnce(() => first.promise)
    .mockImplementationOnce(() => second.promise);
  await mount();

  act(() => {
    app.updateSettings({ theme: "light" });
    app.updateSettings({ privacy_mode: true });
  });
  await waitFor(() => expect(api.updateSettings).toHaveBeenCalledTimes(1));
  const latest: Settings = { ...initial, theme: "light", privacy_mode: true };
  expect(client.getQueryData(["settings"])).toEqual(latest);

  await act(async () => first.resolve({ ...initial, theme: "light" }));
  await waitFor(() => expect(api.updateSettings).toHaveBeenCalledTimes(2));
  expect(api.updateSettings).toHaveBeenLastCalledWith(latest, expect.anything());
  expect(client.getQueryData(["settings"])).toEqual(latest);
  await act(async () => second.resolve(latest));
  await waitFor(() => expect(app.settings).toEqual(latest));
});

it("reloads persisted settings when the last optimistic save fails", async () => {
  const save = deferred<Settings>();
  vi.mocked(api.updateSettings).mockImplementationOnce(() => save.promise);
  await mount();
  act(() => app.updateSettings({ theme: "light" }));
  await waitFor(() => expect(app.settings?.theme).toBe("light"));
  await act(async () => save.reject(new Error("save failed")));
  await waitFor(() => expect(app.settings).toEqual(initial));
  expect(api.getSettings).toHaveBeenCalledTimes(2);
});

it("clears active and inactive data from the old profile before the new reads finish", async () => {
  let switched = false;
  const next = deferred<string>();
  const read = vi.fn(() => (switched ? next.promise : Promise.resolve("old private balance")));
  vi.mocked(api.switchProfile).mockImplementation(async () => {
    switched = true;
    return "real";
  });
  await mount(read);
  await screen.findByText("old private balance");
  client.setQueryData(["inactive-leg"], "old private activity");

  let switching!: Promise<void>;
  act(() => {
    switching = app.switchProfile("real");
  });
  await waitFor(() => expect(read).toHaveBeenCalledTimes(2));
  expect(screen.queryByText("old private balance")).not.toBeInTheDocument();
  expect(client.getQueryData(["inactive-leg"])).toBeUndefined();
  await act(async () => {
    next.resolve("new balance");
    await switching;
  });
  expect(await screen.findByText("new balance")).toBeInTheDocument();
});

it("keeps the current profile data when a profile switch is rejected", async () => {
  vi.mocked(api.switchProfile).mockRejectedValueOnce(new Error("cannot open profile"));
  await mount(() => Promise.resolve("current balance"));
  await screen.findByText("current balance");
  await act(async () => {
    await expect(app.switchProfile("real")).rejects.toThrow("cannot open profile");
  });
  expect(screen.getByText("current balance")).toBeInTheDocument();
});

it("does not let an old in-flight read repopulate data after a profile switch", async () => {
  const old = deferred<string>();
  const read = vi
    .fn()
    .mockImplementationOnce(() => old.promise)
    .mockResolvedValue("new balance");
  vi.mocked(api.switchProfile).mockResolvedValueOnce("real");
  await mount(read);
  await act(async () => app.switchProfile("real"));
  expect(await screen.findByText("new balance")).toBeInTheDocument();
  await act(async () => old.resolve("old private balance"));
  expect(screen.queryByText("old private balance")).not.toBeInTheDocument();
  expect(client.getQueryData(["profile-value"])).toBe("new balance");
});

it("keeps a queued optimistic edit when an earlier save fails", async () => {
  const first = deferred<Settings>();
  const second = deferred<Settings>();
  vi.mocked(api.updateSettings)
    .mockImplementationOnce(() => first.promise)
    .mockImplementationOnce(() => second.promise);
  await mount();
  act(() => {
    app.updateSettings({ theme: "light" });
    app.updateSettings({ privacy_mode: true });
  });
  const latest: Settings = { ...initial, theme: "light", privacy_mode: true };
  await act(async () => first.reject(new Error("first save failed")));
  await waitFor(() => expect(api.updateSettings).toHaveBeenCalledTimes(2));
  expect(client.getQueryData(["settings"])).toEqual(latest);
  expect(api.getSettings).toHaveBeenCalledTimes(1);
  await act(async () => second.resolve(latest));
  await waitFor(() => expect(app.settings).toEqual(latest));
});

it("masks private values until the profile's privacy setting is loaded", async () => {
  const settings = deferred<Settings>();
  vi.mocked(api.getSettings).mockImplementationOnce(() => settings.promise);
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <AppProvider>
        <Probe />
      </AppProvider>
    </QueryClientProvider>,
  );
  expect(app.settings).toBeUndefined();
  expect(app.privacy).toBe(true);
  await act(async () => settings.resolve(initial));
  await waitFor(() => expect(app.privacy).toBe(false));
});

it("finishes queued settings saves in the current profile before switching stores", async () => {
  const save = deferred<Settings>();
  vi.mocked(api.updateSettings).mockImplementationOnce(() => save.promise);
  vi.mocked(api.switchProfile).mockResolvedValueOnce("real");
  await mount();
  act(() => app.updateSettings({ theme: "light" }));
  await waitFor(() => expect(api.updateSettings).toHaveBeenCalledTimes(1));
  let switching!: Promise<void>;
  await act(async () => {
    switching = app.switchProfile("real");
  });
  expect(api.switchProfile).not.toHaveBeenCalled();
  await act(async () => {
    save.resolve({ ...initial, theme: "light" });
    await switching;
  });
  expect(api.switchProfile).toHaveBeenCalledOnce();
  await waitFor(() => expect(app.settings).toEqual(initial));
});
