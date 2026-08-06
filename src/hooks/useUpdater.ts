import { useCallback, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdaterStatus = "idle" | "checking" | "upToDate" | "downloading" | "installing" | "error";

// Shared by UpdateBanner (auto-checks once on launch) and the Settings
// dialog's "Check for updates" button (checks on demand) — each call site
// gets its own independent instance/state, which is fine since they're two
// separate entry points to the same underlying capability.
export function useUpdater() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [status, setStatus] = useState<UpdaterStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  const checkNow = useCallback(async () => {
    setStatus("checking");
    setError(null);
    try {
      // `check()` calls into Tauri's invoke() under the hood, which throws
      // synchronously (not a rejected promise) outside a real Tauri webview —
      // this try/catch covers that case the same as a real network/API error.
      const result = await check();
      setUpdate(result);
      setStatus(result ? "idle" : "upToDate");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setStatus("error");
    }
  }, []);

  const install = useCallback(async () => {
    if (!update) return;
    setStatus("downloading");
    setError(null);
    try {
      await update.downloadAndInstall();
      setStatus("installing");
      await relaunch();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setStatus("error");
    }
  }, [update]);

  const dismiss = useCallback(() => {
    setUpdate(null);
    setStatus("idle");
  }, []);

  return { update, status, error, checkNow, install, dismiss };
}
