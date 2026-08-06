import { useEffect, useMemo, useRef, useState } from "react";
import type { ActorKind, NodeSummaryDto, NodeType } from "../types/node";
import { AuthorBadge } from "./AuthorBadge";

const NODE_TYPES: NodeType[] = ["page", "wiki", "journal", "project", "index", "decision", "research"];
const ACTORS: ActorKind[] = ["user", "ai", "system"];
const ACTOR_LABEL: Record<ActorKind, string> = { user: "Me", ai: "AI (Baymax)", system: "Imported" };

interface TreeNode {
  name: string;
  path: string;
  children: Map<string, TreeNode>;
  items: NodeSummaryDto[];
}

function buildTree(items: NodeSummaryDto[]): TreeNode {
  const root: TreeNode = { name: "", path: "", children: new Map(), items: [] };
  for (const item of items) {
    const segments = (item.folder ?? "").split("/").filter(Boolean);
    if (segments.length === 0) {
      root.items.push(item);
      continue;
    }
    let node = root;
    let pathSoFar = "";
    for (const seg of segments) {
      pathSoFar = pathSoFar ? `${pathSoFar}/${seg}` : seg;
      let child = node.children.get(seg);
      if (!child) {
        child = { name: seg, path: pathSoFar, children: new Map(), items: [] };
        node.children.set(seg, child);
      }
      node = child;
    }
    node.items.push(item);
  }
  return root;
}

function countAll(node: TreeNode): number {
  let n = node.items.length;
  for (const child of node.children.values()) n += countAll(child);
  return n;
}

function collectFolderPaths(node: TreeNode, out: string[] = []): string[] {
  if (node.path) out.push(node.path);
  for (const child of node.children.values()) collectFolderPaths(child, out);
  return out;
}

interface FolderTreeProps {
  items: NodeSummaryDto[];
  nodeType: NodeType | undefined;
  onNodeTypeChange: (type: NodeType | undefined) => void;
  actor: ActorKind | undefined;
  onActorChange: (actor: ActorKind | undefined) => void;
  selectedId: string | null;
  onSelect: (id: string) => void;
  loading: boolean;
  /** Creates a new note, optionally pre-assigned to a folder path. */
  onCreateNote: (folderPath?: string) => void | Promise<void>;
}

// Virtual folders, same principle as node_type filtering: a `folder` property
// (freeform, slash-separated) drives grouping — nothing lives on disk in
// real directories anywhere. A "folder" only exists as long as at least one
// note references it — there's no separate folders table — so "creating a
// folder" really means "creating the first note in it."
export function FolderTree({
  items,
  nodeType,
  onNodeTypeChange,
  actor,
  onActorChange,
  selectedId,
  onSelect,
  loading,
  onCreateNote,
}: FolderTreeProps) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [filterOpen, setFilterOpen] = useState(false);
  const tree = useMemo(() => buildTree(items), [items]);
  const allFolderPaths = useMemo(() => collectFolderPaths(tree), [tree]);
  const filterSummary = `${nodeType ?? "All types"} · ${actor ? ACTOR_LABEL[actor] : "Anyone"}`;

  // Everything starts collapsed on launch — fires once, the first time real
  // folder data shows up, not on every later refresh (which would keep
  // stomping on folders the user deliberately expanded).
  const appliedInitialCollapse = useRef(false);
  useEffect(() => {
    if (appliedInitialCollapse.current || allFolderPaths.length === 0) return;
    appliedInitialCollapse.current = true;
    setCollapsed(new Set(allFolderPaths));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [allFolderPaths]);

  const allCollapsed = allFolderPaths.length > 0 && allFolderPaths.every((p) => collapsed.has(p));

  function toggle(path: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  function toggleAll() {
    setCollapsed(allCollapsed ? new Set() : new Set(allFolderPaths));
  }

  function renderNode(node: TreeNode, depth: number): React.ReactNode {
    const isCollapsed = collapsed.has(node.path);
    return (
      <div key={node.path || "__root_items__"}>
        {node.path && (
          <div className="folder-tree-folder-row" style={{ paddingLeft: `${depth * 14}px` }}>
            <button className="folder-tree-folder" onClick={() => toggle(node.path)}>
              <span className={`folder-tree-caret${isCollapsed ? " collapsed" : ""}`}>▾</span>
              <span className="folder-tree-folder-name">{node.name}</span>
              <span className="folder-tree-folder-count">{countAll(node)}</span>
            </button>
            <button
              className="folder-tree-add-btn"
              onClick={() => onCreateNote(node.path)}
              title={`New note in ${node.path}`}
            >
              +
            </button>
          </div>
        )}
        {!isCollapsed && (
          <>
            {[...node.children.values()]
              .sort((a, b) => a.name.localeCompare(b.name))
              .map((child) => renderNode(child, node.path ? depth + 1 : depth))}
            {node.items
              .sort((a, b) => (a.updated_at < b.updated_at ? 1 : -1))
              .map((item) => (
                <button
                  key={item.id}
                  className={`folder-tree-item${item.id === selectedId ? " selected" : ""}`}
                  style={{ paddingLeft: `${8 + (node.path ? depth + 1 : depth) * 14}px` }}
                  onClick={() => onSelect(item.id)}
                  title={item.title}
                >
                  <span className="folder-tree-item-meta">
                    <AuthorBadge actor={item.created_by} />
                    <span className="folder-tree-item-type">{item.node_type}</span>
                  </span>
                  <span className="folder-tree-item-title">{item.title}</span>
                </button>
              ))}
          </>
        )}
      </div>
    );
  }

  return (
    <div className="folder-tree">
      <div className="folder-tree-filter-bar">
        <button
          type="button"
          className="folder-tree-filter-btn"
          onClick={() => setFilterOpen((o) => !o)}
        >
          <span>Filter</span>
          <span className="folder-tree-filter-summary">{filterSummary}</span>
        </button>
        {filterOpen && (
          <>
            <div className="folder-tree-filter-scrim" onClick={() => setFilterOpen(false)} />
            <div className="folder-tree-filter-popover">
              <select
                className="folder-tree-filter"
                value={nodeType ?? ""}
                onChange={(e) => onNodeTypeChange((e.target.value || undefined) as NodeType | undefined)}
              >
                <option value="">All types</option>
                {NODE_TYPES.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
              <select
                className="folder-tree-filter"
                value={actor ?? ""}
                onChange={(e) => onActorChange((e.target.value || undefined) as ActorKind | undefined)}
              >
                <option value="">Written by anyone</option>
                {ACTORS.map((a) => (
                  <option key={a} value={a}>
                    {ACTOR_LABEL[a]}
                  </option>
                ))}
              </select>
            </div>
          </>
        )}
      </div>
      <div className="folder-tree-actions">
        <button
          onClick={() => {
            const name = window.prompt("New folder name (e.g. Fitness/Nutrition):");
            const trimmed = name?.trim();
            if (trimmed) onCreateNote(trimmed);
          }}
        >
          + Folder
        </button>
        <button
          onClick={toggleAll}
          disabled={allFolderPaths.length === 0}
          title={allCollapsed ? "Expand all folders" : "Collapse all folders"}
        >
          {allCollapsed ? "Expand all" : "Collapse all"}
        </button>
      </div>
      <div className="folder-tree-list">
        {loading && <div className="folder-tree-empty">Loading…</div>}
        {!loading && items.length === 0 && <div className="folder-tree-empty">No notes yet</div>}
        {!loading && items.length > 0 && renderNode(tree, 0)}
      </div>
    </div>
  );
}
