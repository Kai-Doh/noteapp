import { useUpdater } from "../hooks/useUpdater";

export function UpdateSettingsSection() {
  const { update, status, error, checkNow, install } = useUpdater();

  return (
    <div className="settings-section">
      <h2 className="settings-section-title">Updates</h2>
      {update ? (
        <div className="settings-update-row">
          <span>Update {update.version} is available.</span>
          <button
            type="button"
            onClick={install}
            disabled={status === "downloading" || status === "installing"}
          >
            {status === "downloading" ? "Downloading…" : status === "installing" ? "Restarting…" : "Install & Restart"}
          </button>
        </div>
      ) : (
        <div className="settings-update-row">
          <span className="settings-update-status">
            {status === "checking" && "Checking…"}
            {status === "upToDate" && "You're up to date."}
            {status === "error" && `Check failed: ${error}`}
            {status === "idle" && "Check whether a newer version has been published."}
          </span>
          <button type="button" onClick={checkNow} disabled={status === "checking"}>
            Check for updates
          </button>
        </div>
      )}
    </div>
  );
}
