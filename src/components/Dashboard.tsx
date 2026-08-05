import { useState, type FormEvent } from "react";
import type { NodeSummaryDto } from "../types/node";
import { AuthorBadge } from "./AuthorBadge";

interface DashboardProps {
  items: NodeSummaryDto[];
  onSelect: (id: string) => void;
  onQuickCapture: (title: string) => Promise<void>;
}

export function Dashboard({ items, onSelect, onQuickCapture }: DashboardProps) {
  const [draft, setDraft] = useState("");
  const [creating, setCreating] = useState(false);

  const recent = [...items].sort((a, b) => (a.updated_at < b.updated_at ? 1 : -1)).slice(0, 15);

  async function submit(e: FormEvent) {
    e.preventDefault();
    const title = draft.trim();
    if (!title || creating) return;
    setCreating(true);
    try {
      await onQuickCapture(title);
      setDraft("");
    } finally {
      setCreating(false);
    }
  }

  return (
    <div className="dashboard">
      <h1>Notes</h1>
      <form className="quick-capture" onSubmit={submit}>
        <input
          placeholder="Quick capture — note title, Enter to create"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          autoFocus
        />
        <button type="submit" disabled={!draft.trim() || creating}>
          New
        </button>
      </form>

      <h2>Recently updated</h2>
      <ul className="dashboard-list">
        {recent.map((item) => (
          <li key={item.id}>
            <button className="dashboard-list-item" onClick={() => onSelect(item.id)}>
              <span className="dashboard-list-left">
                <AuthorBadge actor={item.created_by} />
                <span className="dashboard-list-title">{item.title}</span>
              </span>
              <span className="dashboard-list-meta">
                {item.node_type} · {new Date(item.updated_at).toLocaleString()}
              </span>
            </button>
          </li>
        ))}
        {recent.length === 0 && <li className="folder-tree-empty">Nothing yet — create your first note above.</li>}
      </ul>
    </div>
  );
}
