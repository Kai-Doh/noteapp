import { useEffect, useState } from "react";
import { listChangelog } from "../api/review";
import type { ChangelogEntryDto } from "../types/memory";

export function AiActivityFeed() {
  const [entries, setEntries] = useState<ChangelogEntryDto[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    listChangelog("ai", 100)
      .then((res) => {
        if (!cancelled) setEntries(res.items);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="ai-activity-feed">
      <h2>AI Activity</h2>
      {loading && <div className="panel-empty">Loading…</div>}
      {!loading && entries.length === 0 && <div className="panel-empty">No AI-attributed writes yet.</div>}
      <ul className="activity-list">
        {entries.map((entry) => (
          <li key={entry.id} className="activity-item">
            <div className="activity-item-meta">
              <span className={`activity-action activity-action-${entry.action}`}>{entry.action}</span>
              <span className="activity-entity">
                {entry.entity_type} · {entry.entity_id.slice(0, 8)}…
              </span>
              {entry.compiler_version && <span className="activity-compiler">compiler v{entry.compiler_version}</span>}
              <span className="activity-timestamp">{new Date(entry.timestamp).toLocaleString()}</span>
            </div>
            {entry.reason && <p className="activity-reason">{entry.reason}</p>}
            <pre className="activity-diff">{JSON.stringify(entry.diff_json, null, 2)}</pre>
          </li>
        ))}
      </ul>
    </div>
  );
}
