import { useEffect, useState } from "react";
import { getBacklinks } from "../api/nodes";
import type { BacklinkDto } from "../types/node";

interface BacklinksPanelProps {
  nodeId: string;
  onSelect: (id: string) => void;
}

export function BacklinksPanel({ nodeId, onSelect }: BacklinksPanelProps) {
  const [backlinks, setBacklinks] = useState<BacklinkDto[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getBacklinks(nodeId)
      .then((res) => {
        if (!cancelled) setBacklinks(res.items);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [nodeId]);

  return (
    <div className="backlinks-panel">
      <h3>Backlinks</h3>
      {loading && <div className="panel-empty">Loading…</div>}
      {!loading && backlinks.length === 0 && <div className="panel-empty">No notes link here yet.</div>}
      <ul>
        {backlinks.map((b) => (
          <li key={b.source_node_id}>
            <button className="backlink-item" onClick={() => onSelect(b.source_node_id)}>
              {b.source_title}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
