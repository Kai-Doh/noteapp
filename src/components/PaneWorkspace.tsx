import { useEffect, useRef, useState } from "react";
import type { LeafPane, PaneNode, SplitEdge } from "../state/paneTree";
import type { NodeSummaryDto } from "../types/node";

const DRAG_MIME = "application/x-noteapp-tab";

interface DragPayload {
  nodeId: string;
  sourcePaneId: string;
}

interface PaneWorkspaceProps {
  tree: PaneNode;
  activePaneId: string;
  items: NodeSummaryDto[];
  renderLeafContent: (pane: LeafPane) => React.ReactNode;
  onFocusPane: (paneId: string) => void;
  onActivateTab: (paneId: string, nodeId: string) => void;
  onCloseTab: (paneId: string, nodeId: string) => void;
  onMoveTab: (nodeId: string, sourcePaneId: string, targetPaneId: string) => void;
  onSplitWithTab: (targetPaneId: string, edge: SplitEdge, nodeId: string, sourcePaneId: string) => void;
  onResize: (splitId: string, ratio: number) => void;
}

export function PaneWorkspace(props: PaneWorkspaceProps) {
  return <PaneNodeView {...props} />;
}

function PaneNodeView(props: PaneWorkspaceProps) {
  const { tree } = props;
  if (tree.type === "split") {
    return <SplitPaneView {...props} tree={tree} />;
  }
  return <LeafPaneView {...props} pane={tree} />;
}

function SplitPaneView(props: PaneWorkspaceProps & { tree: Extract<PaneNode, { type: "split" }> }) {
  const { tree, onResize } = props;
  const containerRef = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    if (!dragging) return;
    function onMove(e: MouseEvent) {
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const ratio =
        tree.dir === "row" ? (e.clientX - rect.left) / rect.width : (e.clientY - rect.top) / rect.height;
      onResize(tree.id, ratio);
    }
    function onUp() {
      setDragging(false);
    }
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [dragging, tree.dir, tree.id, onResize]);

  return (
    <div ref={containerRef} className={`pane-split pane-split-${tree.dir}`}>
      <div className="pane-split-child" style={{ flexGrow: tree.ratio, flexBasis: 0 }}>
        <PaneNodeView {...props} tree={tree.a} />
      </div>
      <div
        className={`pane-split-resizer pane-split-resizer-${tree.dir}`}
        onMouseDown={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
      />
      <div className="pane-split-child" style={{ flexGrow: 1 - tree.ratio, flexBasis: 0 }}>
        <PaneNodeView {...props} tree={tree.b} />
      </div>
    </div>
  );
}

function zoneFromPointer(rect: DOMRect, x: number, y: number): SplitEdge | "center" {
  const relX = (x - rect.left) / rect.width;
  const relY = (y - rect.top) / rect.height;
  if (relX < 0.22) return "left";
  if (relX > 0.78) return "right";
  if (relY < 0.22) return "top";
  if (relY > 0.78) return "bottom";
  return "center";
}

function LeafPaneView({
  pane,
  activePaneId,
  items,
  renderLeafContent,
  onFocusPane,
  onActivateTab,
  onCloseTab,
  onMoveTab,
  onSplitWithTab,
}: PaneWorkspaceProps & { pane: LeafPane }) {
  const [dragZone, setDragZone] = useState<SplitEdge | "center" | null>(null);
  const isActive = pane.id === activePaneId;

  function handleDragOver(e: React.DragEvent) {
    if (!e.dataTransfer.types.includes(DRAG_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const rect = e.currentTarget.getBoundingClientRect();
    setDragZone(zoneFromPointer(rect, e.clientX, e.clientY));
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    // Computed fresh from the drop event itself rather than read from
    // `dragZone` state — that state is set by `dragover` asynchronously, and
    // a fast drag-release can fire `drop` before React commits the render
    // that would have made the latest zone visible to this closure.
    const rect = e.currentTarget.getBoundingClientRect();
    const zone = zoneFromPointer(rect, e.clientX, e.clientY);
    setDragZone(null);
    const raw = e.dataTransfer.getData(DRAG_MIME);
    if (!raw) return;
    const payload = JSON.parse(raw) as DragPayload;
    if (!zone || zone === "center") {
      onMoveTab(payload.nodeId, payload.sourcePaneId, pane.id);
    } else {
      onSplitWithTab(pane.id, zone, payload.nodeId, payload.sourcePaneId);
    }
  }

  return (
    <div
      className={`pane-leaf${isActive ? " active" : ""}`}
      onMouseDown={() => onFocusPane(pane.id)}
      onDragOver={handleDragOver}
      onDragLeave={() => setDragZone(null)}
      onDrop={handleDrop}
    >
      <div className="pane-tabbar">
        {pane.tabs.map((id) => {
          const item = items.find((i) => i.id === id);
          return (
            <div
              key={id}
              className={`pane-tab${id === pane.activeId ? " active" : ""}`}
              draggable
              onDragStart={(e) => {
                e.dataTransfer.setData(DRAG_MIME, JSON.stringify({ nodeId: id, sourcePaneId: pane.id }));
                e.dataTransfer.effectAllowed = "move";
              }}
              onClick={() => onActivateTab(pane.id, id)}
              title={item?.title ?? "Untitled"}
            >
              <span className="pane-tab-title">{item?.title ?? "Untitled"}</span>
              <button
                type="button"
                className="pane-tab-close"
                onClick={(e) => {
                  e.stopPropagation();
                  onCloseTab(pane.id, id);
                }}
                title="Close"
              >
                ×
              </button>
            </div>
          );
        })}
      </div>
      <div className="pane-leaf-content">
        {renderLeafContent(pane)}
        {dragZone && (
          <div className={`pane-drop-overlay pane-drop-overlay-${dragZone}`} />
        )}
      </div>
    </div>
  );
}
