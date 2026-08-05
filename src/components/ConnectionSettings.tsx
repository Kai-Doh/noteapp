import { useEffect, useState, type FormEvent } from "react";
import { getServerConfig, getServerToken, setServerConfig } from "../api/connection";
import { apiFetch } from "../api/client";

interface ConnectionSettingsProps {
  onConnected: () => void;
  onCancel?: () => void;
}

export function ConnectionSettings({ onConnected, onCancel }: ConnectionSettingsProps) {
  const [url, setUrl] = useState("");
  const [token, setToken] = useState("");
  const [hadExistingConfig, setHadExistingConfig] = useState(false);
  const [hasToken, setHasToken] = useState(false);
  const [status, setStatus] = useState<"idle" | "testing" | "error">("idle");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getServerConfig()
      .then((cfg) => {
        setHasToken(cfg.has_token);
        setHadExistingConfig(Boolean(cfg.url));
        if (cfg.url) setUrl(cfg.url);
      })
      .catch(() => {
        // No existing config to prefill from — the form's own empty
        // defaults are the correct fallback, nothing more to do here.
      });
  }, []);

  async function handleSave(e: FormEvent) {
    e.preventDefault();
    setStatus("testing");
    setError(null);
    try {
      const cleanUrl = url.trim().replace(/\/+$/, "");
      // Leaving the token field blank keeps whatever's already stored, rather
      // than wiping it — only relevant when just changing the URL.
      const tokenToSave = token.trim() || (hasToken ? await getServerToken() : null);
      if (!tokenToSave) {
        throw new Error("A token is required.");
      }
      await setServerConfig(cleanUrl, tokenToSave);
      // Verify the config actually works before declaring success.
      await apiFetch("/nodes?limit=1");
      onConnected();
    } catch (err) {
      setStatus("error");
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div className="connection-settings">
      <div className="connection-settings-card">
        <h1>Connect to your vault</h1>
        <p className="connection-settings-hint">
          Enter the noteapp server's address and the <code>desktop</code> token printed in its
          startup logs (<code>docker logs &lt;container&gt;</code> if it's running in Docker).
        </p>
        <form onSubmit={handleSave}>
          <label>
            Server URL
            <input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="http://100.x.x.x:47823"
              required
            />
          </label>
          <label>
            Token
            <input
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder={hasToken ? "•••••••••••••••• (leave blank to keep current)" : "paste the 'desktop' token"}
            />
          </label>
          {error && <p className="connection-settings-error">{error}</p>}
          <div className="connection-settings-actions">
            <button type="submit" disabled={status === "testing"}>
              {status === "testing" ? "Connecting…" : "Connect"}
            </button>
            {onCancel && hadExistingConfig && (
              <button type="button" onClick={onCancel}>
                Cancel
              </button>
            )}
          </div>
        </form>
      </div>
    </div>
  );
}
