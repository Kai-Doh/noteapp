import { useEffect, useRef, useState } from "react";
import type { LeafPane, PaneNode, SplitEdge } from "../state/paneTree";
import type { NodeSummaryDto } from "../types/node";

// Tab dragging is implemented with plain mouse events rather than native
// HTML5 drag-and-drop — the latter is unreliable inside Tauri's WebView2
// (drags often never start, or start but never fire a drop), even though it
// works fine in a regular browser tab. Mouse events are engine-agnostic and
// already proven out by the split resizer below.
const DRAG_THRESHOLD_PX = 5;

interface DragInfo {
  nodeId: string;
  sourcePaneId: string;
}

interface HoverInfo {
  paneId: string;
  zone: SplitEdge | "center";
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

function resolveHover(x: number, y: number): HoverInfo | null {
  const el = document.elementFromPoint(x, y);
  const paneEl = el instanceof Element ? el.closest<HTMLElement>("[data-pane-id]") : null;
  if (!paneEl) return null;
  const rect = paneEl.getBoundingClientRect();
  return { paneId: paneEl.dataset.paneId!, zone: zoneFromPointer(rect, x, y) };
}

export function PaneWorkspace(props: PaneWorkspaceProps) {
  const [drag, setDrag] = useState<DragInfo | null>(null);
  const [hover, setHover] = useState<HoverInfo | null>(null);
  const dragRef = useRef<DragInfo | null>(null);
  const startRef = useRef<{ nodeId: string; sourcePaneId: string; x: number; y: number } | null>(null);
  const propsRef = useRef(props);
  propsRef.current = props;

  const beginPossibleDrag = (nodeId: string, sourcePaneId: string, x: number, y: number) => {
    startRef.current = { nodeId, sourcePaneId, x, y };
  };

  useEffect(() => {
    function onMove(e: MouseEvent) {
      if (!dragRef.current) {
        const start = startRef.current;
        if (!start) return;
        if (Math.hypot(e.clientX - start.x, e.clientY - start.y) < DRAG_THRESHOLD_PX) return;
        const info = { nodeId: start.nodeId, sourcePaneId: start.sourcePaneId };
        dragRef.current = info;
        setDrag(info);
        document.body.classList.add("pane-dragging");
      }
      setHover(resolveHover(e.clientX, e.clientY));
    }

    function onUp(e: MouseEvent) {
      const activeDrag = dragRef.current;
      // Resolved fresh from the mouseup event's own coordinates rather than
      // trusting whatever `hover` state the last `mousemove` set — some
      // input sources (automated drags, very fast releases) fire far fewer
      // move events than a real mouse, so the last-known hover can be stale.
      const activeHover = activeDrag ? resolveHover(e.clientX, e.clientY) : null;
      if (activeDrag && activeHover) {
        if (activeHover.zone === "center") {
          propsRef.current.onMoveTab(activeDrag.nodeId, activeDrag.sourcePaneId, activeHover.paneId);
        } else {
          propsRef.current.onSplitWithTab(
            activeHover.paneId,
            activeHover.zone,
            activeDrag.nodeId,
            activeDrag.sourcePaneId,
          );
        }
      }
      startRef.current = null;
      dragRef.current = null;
      setDrag(null);
      setHover(null);
      document.body.classList.remove("pane-dragging");
    }

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  return (
    <PaneNodeView
      {...props}
      tree={props.tree}
      dragActive={!!drag}
      hoverPaneId={hover?.paneId ?? null}
      hoverZone={hover?.zone ?? null}
      onTabMouseDown={beginPossibleDrag}
    />
  );
}

interface RenderProps extends PaneWorkspaceProps {
  dragActive: boolean;
  hoverPaneId: string | null;
  hoverZone: SplitEdge | "center" | null;
  onTabMouseDown: (nodeId: string, sourcePaneId: string, x: number, y: number) => void;
}

function PaneNodeView(props: RenderProps) {
  const { tree } = props;
  if (tree.type === "split") {
    return <SplitPaneView {...props} tree={tree} />;
  }
  return <LeafPaneView {...props} pane={tree} />;
}

function SplitPaneView(props: RenderProps & { tree: Extract<PaneNode, { type: "split" }> }) {
  const { tree, onResize } = props;
  const containerRef = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);
  // Starts collapsed and grows into its real ratio right after mount, so a
  // freshly created split visibly expands into place instead of snapping in
  // at full size. Only fires once per split's lifetime — later resizer drags
  // update `tree.ratio` on the same, already-mounted instance, so this
  // doesn't replay then (and the transition is suppressed while actively
  // dragging so the divider still tracks the pointer 1:1).
  const [grown, setGrown] = useState(false);
  useEffect(() => {
    const raf = requestAnimationFrame(() => setGrown(true));
    return () => cancelAnimationFrame(raf);
  }, []);

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

  const animated = !dragging;
  return (
    <div ref={containerRef} className={`pane-split pane-split-${tree.dir}`}>
      <div
        className={`pane-split-child${animated ? " pane-split-child-animated" : ""}`}
        style={{ flexGrow: grown ? tree.ratio : 0, flexBasis: 0 }}
      >
        <PaneNodeView {...props} tree={tree.a} />
      </div>
      <div
        className={`pane-split-resizer pane-split-resizer-${tree.dir}`}
        onMouseDown={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
      />
      <div
        className={`pane-split-child${animated ? " pane-split-child-animated" : ""}`}
        style={{ flexGrow: grown ? 1 - tree.ratio : 0, flexBasis: 0 }}
      >
        <PaneNodeView {...props} tree={tree.b} />
      </div>
    </div>
  );
}

// Fixed pixel margins rather than a percentage of the pane's size — a
// percentage-based edge band goes razor-thin (and hard to actually land on)
// on a short or narrow pane, especially once several splits exist. Capped at
// 35% of the dimension so a small pane doesn't lose its center entirely.
const EDGE_ZONE_PX = 44;

function zoneFromPointer(rect: DOMRect, x: number, y: number): SplitEdge | "center" {
  const marginX = Math.min(EDGE_ZONE_PX, rect.width * 0.35);
  const marginY = Math.min(EDGE_ZONE_PX, rect.height * 0.35);
  const relX = x - rect.left;
  const relY = y - rect.top;
  if (relX < marginX) return "left";
  if (relX > rect.width - marginX) return "right";
  if (relY < marginY) return "top";
  if (relY > rect.height - marginY) return "bottom";
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
  dragActive,
  hoverPaneId,
  hoverZone,
  onTabMouseDown,
}: RenderProps & { pane: LeafPane }) {
  const isActive = pane.id === activePaneId;
  const isHovered = dragActive && hoverPaneId === pane.id;

  return (
    <div
      className={`pane-leaf${isActive ? " active" : ""}`}
      data-pane-id={pane.id}
      onMouseDown={() => onFocusPane(pane.id)}
    >
      <div className="pane-tabbar">
        {pane.tabs.map((id) => {
          const item = items.find((i) => i.id === id);
          return (
            <div
              key={id}
              className={`pane-tab${id === pane.activeId ? " active" : ""}`}
              onMouseDown={(e) => {
                if (e.button !== 0) return;
                onTabMouseDown(id, pane.id, e.clientX, e.clientY);
              }}
              onClick={() => onActivateTab(pane.id, id)}
              title={item?.title ?? "Untitled"}
            >
              <span className="pane-tab-title">{item?.title ?? "Untitled"}</span>
              <button
                type="button"
                className="pane-tab-close"
                onMouseDown={(e) => e.stopPropagation()}
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
        {isHovered && hoverZone && (
          <div className={`pane-drop-overlay pane-drop-overlay-${hoverZone}`} />
        )}
      </div>
    </div>
  );
}
