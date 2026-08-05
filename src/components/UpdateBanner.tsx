import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type Status = "idle" | "downloading" | "installing" | "error";

export function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Deferred + caught, not called bare: `check()` calls into Tauri's
    // invoke() under the hood, which throws synchronously (not a rejected
    // promise) outside a real Tauri webview — and a missing/unpublished
    // update endpoint is an expected, silent outcome either way.
    Promise.resolve()
      .then(() => check())
      .then((result) => setUpdate(result))
      .catch(() => {});
  }, []);

  if (!update) return null;

  const handleInstall = async () => {
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
  };

  return (
    <div className="update-banner">
      <span>Update {update.version} is available.</span>
      {status === "idle" && (
        <button className="update-banner-install" onClick={handleInstall}>
          Install &amp; Restart
        </button>
      )}
      {status === "downloading" && <span className="update-banner-status">Downloading…</span>}
      {status === "installing" && <span className="update-banner-status">Restarting…</span>}
      {status === "error" && <span className="update-banner-error">Update failed: {error}</span>}
      {status === "idle" && (
        <button className="update-banner-dismiss" onClick={() => setUpdate(null)}>
          Dismiss
        </button>
      )}
    </div>
  );
}
