// A binary tree of editor panes, VS-Code-style: leaves hold an ordered list
// of open tabs (note ids) plus which one is active; splits hold two children
// side by side (row) or stacked (col) with a resizable ratio between them.
// Dragging a tab onto another pane's center moves it there; dragging onto an
// edge splits that pane and drops the tab into the new half.

export interface LeafPane {
  type: "leaf";
  id: string;
  tabs: string[];
  activeId: string | null;
}

export interface SplitPane {
  type: "split";
  id: string;
  dir: "row" | "col";
  a: PaneNode;
  b: PaneNode;
  ratio: number;
}

export type PaneNode = LeafPane | SplitPane;
export type SplitEdge = "left" | "right" | "top" | "bottom";

let counter = 0;
function newPaneId(): string {
  counter += 1;
  return `pane-${Date.now().toString(36)}-${counter}`;
}

export function createLeaf(): LeafPane {
  return { type: "leaf", id: newPaneId(), tabs: [], activeId: null };
}

export function collectLeaves(node: PaneNode, out: LeafPane[] = []): LeafPane[] {
  if (node.type === "leaf") {
    out.push(node);
  } else {
    collectLeaves(node.a, out);
    collectLeaves(node.b, out);
  }
  return out;
}

export function findLeaf(node: PaneNode, id: string): LeafPane | null {
  if (node.type === "leaf") return node.id === id ? node : null;
  return findLeaf(node.a, id) ?? findLeaf(node.b, id);
}

function mapLeaf(node: PaneNode, id: string, fn: (leaf: LeafPane) => LeafPane): PaneNode {
  if (node.type === "leaf") return node.id === id ? fn(node) : node;
  return { ...node, a: mapLeaf(node.a, id, fn), b: mapLeaf(node.b, id, fn) };
}

export function openInPane(tree: PaneNode, paneId: string, nodeId: string): PaneNode {
  return mapLeaf(tree, paneId, (leaf) => ({
    ...leaf,
    tabs: leaf.tabs.includes(nodeId) ? leaf.tabs : [...leaf.tabs, nodeId],
    activeId: nodeId,
  }));
}

export function setActiveTab(tree: PaneNode, paneId: string, nodeId: string): PaneNode {
  return mapLeaf(tree, paneId, (leaf) => ({ ...leaf, activeId: nodeId }));
}

// Clears the active tab without closing any of them — used by the "Home"
// button to drop back to the dashboard while keeping every open tab around
// to click back into.
export function clearActiveTab(tree: PaneNode, paneId: string): PaneNode {
  return mapLeaf(tree, paneId, (leaf) => ({ ...leaf, activeId: null }));
}

function removeTabFromLeaf(node: PaneNode, paneId: string, nodeId: string): PaneNode {
  return mapLeaf(node, paneId, (leaf) => {
    const tabs = leaf.tabs.filter((t) => t !== nodeId);
    const activeId = leaf.activeId === nodeId ? (tabs[tabs.length - 1] ?? null) : leaf.activeId;
    return { ...leaf, tabs, activeId };
  });
}

// Collapses any split whose child leaf ran out of tabs, promoting its
// sibling up to take its place. The root itself is left alone even if it's
// an empty leaf — callers render a dashboard/empty-state for that case.
function prune(node: PaneNode): PaneNode {
  if (node.type === "leaf") return node;
  const a = prune(node.a);
  const b = prune(node.b);
  if (a.type === "leaf" && a.tabs.length === 0) return b;
  if (b.type === "leaf" && b.tabs.length === 0) return a;
  return { ...node, a, b };
}

export function closeTab(tree: PaneNode, paneId: string, nodeId: string): PaneNode {
  return prune(removeTabFromLeaf(tree, paneId, nodeId));
}

// Removes a note from every pane it's open in — used when the note itself
// gets deleted, so no tab is left pointing at nothing.
export function closeTabEverywhere(tree: PaneNode, nodeId: string): PaneNode {
  function strip(node: PaneNode): PaneNode {
    if (node.type === "leaf") {
      if (!node.tabs.includes(nodeId)) return node;
      const tabs = node.tabs.filter((t) => t !== nodeId);
      const activeId = node.activeId === nodeId ? (tabs[tabs.length - 1] ?? null) : node.activeId;
      return { ...node, tabs, activeId };
    }
    return { ...node, a: strip(node.a), b: strip(node.b) };
  }
  return prune(strip(tree));
}

export function moveTab(tree: PaneNode, fromPaneId: string, toPaneId: string, nodeId: string): PaneNode {
  if (fromPaneId === toPaneId) return setActiveTab(tree, toPaneId, nodeId);
  const removed = closeTab(tree, fromPaneId, nodeId);
  return openInPane(removed, toPaneId, nodeId);
}

// Splits `targetPaneId` along the dropped edge and moves `nodeId` (dragged
// out of `sourcePaneId`) into the freshly created half.
export function splitWithTab(
  tree: PaneNode,
  targetPaneId: string,
  edge: SplitEdge,
  nodeId: string,
  sourcePaneId: string,
): PaneNode {
  const dir: "row" | "col" = edge === "left" || edge === "right" ? "row" : "col";
  const newLeaf: LeafPane = { type: "leaf", id: newPaneId(), tabs: [nodeId], activeId: nodeId };

  function go(node: PaneNode): PaneNode {
    if (node.type === "leaf") {
      if (node.id !== targetPaneId) return node;
      const newFirst = edge === "left" || edge === "top";
      return {
        type: "split",
        id: newPaneId(),
        dir,
        a: newFirst ? newLeaf : node,
        b: newFirst ? node : newLeaf,
        ratio: 0.5,
      };
    }
    return { ...node, a: go(node.a), b: go(node.b) };
  }

  const withSplit = go(tree);
  return prune(removeTabFromLeaf(withSplit, sourcePaneId, nodeId));
}

export function updateRatio(tree: PaneNode, splitId: string, ratio: number): PaneNode {
  if (tree.type === "leaf") return tree;
  if (tree.id === splitId) return { ...tree, ratio: Math.min(0.85, Math.max(0.15, ratio)) };
  return { ...tree, a: updateRatio(tree.a, splitId, ratio), b: updateRatio(tree.b, splitId, ratio) };
}

const STORAGE_KEY = "noteapp.paneTree";

export function loadPaneTree(): PaneNode {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return createLeaf();
    const parsed = JSON.parse(raw) as PaneNode;
    if (!parsed || (parsed.type !== "leaf" && parsed.type !== "split")) return createLeaf();
    return parsed;
  } catch {
    return createLeaf();
  }
}

export function savePaneTree(tree: PaneNode) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(tree));
}
