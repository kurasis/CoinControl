import { isTauri } from "../ipc/client";

/**
 * Opens an https link in the system browser. Inside the app, the Rust-side
 * opener scope only permits allow-listed documentation hosts.
 */
export async function openExternal(url: string): Promise<void> {
  if (!url.startsWith("https://")) return;
  if (isTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}
