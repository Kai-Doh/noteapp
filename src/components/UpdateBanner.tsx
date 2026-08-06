import { useEffect } from "react";
import { useUpdater } from "../hooks/useUpdater";

export function UpdateBanner() {
  const { update, status, error, checkNow, install, dismiss } = useUpdater();

  useEffect(() => {
    checkNow();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!update) return null;

  return (
    <div className="update-banner">
      <span>Update {update.version} is available.</span>
      {status === "idle" && (
        <button className="update-banner-install" onClick={install}>
          Install &amp; Restart
        </button>
      )}
      {status === "downloading" && <span className="update-banner-status">Downloading…</span>}
      {status === "installing" && <span className="update-banner-status">Restarting…</span>}
      {status === "error" && <span className="update-banner-error">Update failed: {error}</span>}
      {status === "idle" && (
        <button className="update-banner-dismiss" onClick={dismiss}>
          Dismiss
        </button>
      )}
    </div>
  );
}
