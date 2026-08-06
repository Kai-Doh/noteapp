import { useEffect, useState } from "react";
import { ConnectionSettings } from "./ConnectionSettings";
import { UpdateSettingsSection } from "./UpdateSettingsSection";

type Category = "server" | "updates";

const CATEGORIES: { key: Category; label: string }[] = [
  { key: "server", label: "Server" },
  { key: "updates", label: "Updates" },
];

interface SettingsDialogProps {
  onConnected: () => void;
  onCancel: () => void;
}

export function SettingsDialog({ onConnected, onCancel }: SettingsDialogProps) {
  const [category, setCategory] = useState<Category>("server");

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onCancel();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onCancel]);

  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <div
        className="dialog settings-dialog"
        style={{ width: "min(640px, 100%)" }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="settings-dialog-header">
          <div className="settings-dialog-title">Settings</div>
          <button type="button" className="dialog-close-btn" onClick={onCancel} title="Close">
            ×
          </button>
        </div>
        <div className="settings-dialog-body">
          <nav className="settings-sidebar">
            {CATEGORIES.map((c) => (
              <button
                key={c.key}
                type="button"
                className={`settings-sidebar-item${category === c.key ? " active" : ""}`}
                onClick={() => setCategory(c.key)}
              >
                {c.label}
              </button>
            ))}
          </nav>
          <div className="settings-content">
            {category === "server" && (
              <ConnectionSettings embedded onConnected={onConnected} onCancel={onCancel} />
            )}
            {category === "updates" && <UpdateSettingsSection />}
          </div>
        </div>
      </div>
    </div>
  );
}
