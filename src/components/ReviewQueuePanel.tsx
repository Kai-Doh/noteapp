import { useCallback, useEffect, useState } from "react";
import { applyReviewItem, approveReviewItem, listReviewItems, rejectReviewItem } from "../api/review";
import type { ReviewItemDto } from "../types/memory";

const STATUS_FILTERS = ["pending", "approved", "rejected", "applied"] as const;

export function ReviewQueuePanel() {
  const [status, setStatus] = useState<(typeof STATUS_FILTERS)[number]>("pending");
  const [items, setItems] = useState<ReviewItemDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listReviewItems(status);
      setItems(res.items);
    } finally {
      setLoading(false);
    }
  }, [status]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function withBusy(id: string, action: () => Promise<unknown>) {
    setBusyId(id);
    try {
      await action();
      await refresh();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <div className="review-queue-panel">
      <div className="review-queue-header">
        <h2>AI Review Queue</h2>
        <select value={status} onChange={(e) => setStatus(e.target.value as typeof status)}>
          {STATUS_FILTERS.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
      </div>

      {loading && <div className="panel-empty">Loading…</div>}
      {!loading && items.length === 0 && <div className="panel-empty">No {status} proposals.</div>}

      <ul className="review-queue-list">
        {items.map((item) => (
          <li key={item.id} className="review-queue-item">
            <div className="review-queue-item-meta">
              <span className={`review-badge review-badge-${item.entity_type}`}>{item.entity_type}</span>
              <span className="review-badge review-badge-action">{item.proposed_action}</span>
              {item.confidence && <span className="review-confidence">confidence: {item.confidence}</span>}
              <span className="review-timestamp">{new Date(item.created_at).toLocaleString()}</span>
            </div>
            {item.reason && <p className="review-reason">{item.reason}</p>}
            <pre className="review-diff">{JSON.stringify(item.proposed_diff_json, null, 2)}</pre>
            {item.status === "applied" && item.applied_changelog_id && (
              <p className="review-applied-note">Applied — changelog {item.applied_changelog_id.slice(0, 8)}…</p>
            )}
            {item.status === "pending" && (
              <div className="review-actions">
                <button disabled={busyId === item.id} onClick={() => withBusy(item.id, () => approveReviewItem(item.id))}>
                  Approve
                </button>
                <button
                  disabled={busyId === item.id}
                  className="review-reject"
                  onClick={() => withBusy(item.id, () => rejectReviewItem(item.id))}
                >
                  Reject
                </button>
              </div>
            )}
            {item.status === "approved" && (
              <div className="review-actions">
                <button disabled={busyId === item.id} onClick={() => withBusy(item.id, () => applyReviewItem(item.id))}>
                  Apply
                </button>
              </div>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
