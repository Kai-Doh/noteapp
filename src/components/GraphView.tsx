import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ForceGraph2D, {
  type NodeObject,
  type LinkObject,
  type ForceGraphMethods,
} from "react-force-graph-2d";
import { getGraph } from "../api/graph";
import type { ActorKind, NodeType } from "../types/node";
import type { GraphFilters } from "../types/graph";
import { ACTOR_COLOR } from "../theme";

const NODE_TYPES: NodeType[] = ["page", "wiki", "journal", "project", "index", "decision", "research"];
const ACTORS: ActorKind[] = ["user", "ai", "system"];

interface GraphNode extends NodeObject {
  id: string;
  title: string;
  node_type: string;
  created_by: string;
  color_key: string;
  hasUnresolved: boolean;
}

interface GraphLink extends LinkObject {
  source: string;
  target: string;
  link_type: string;
  status: string;
}

interface GraphViewProps {
  onSelect: (id: string) => void;
}

export function GraphView({ onSelect }: GraphViewProps) {
  const [filters, setFilters] = useState<GraphFilters>({});
  const [nodes, setNodes] = useState<GraphNode[]>([]);
  const [links, setLinks] = useState<GraphLink[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // react-force-graph-2d's own auto-sizing runs before this flex-based
  // container settles to its final size, leaving the canvas at 0x0 — measure
  // the container ourselves and pass explicit width/height instead of
  // relying on the library's internal ResizeObserver timing.
  const canvasWrapRef = useRef<HTMLDivElement>(null);
  const [dimensions, setDimensions] = useState({ width: 0, height: 0 });
  // force-graph doesn't auto-fit the viewport to where the simulation
  // actually settles — without this, nodes can end up positioned well
  // outside the visible canvas and it just looks blank.
  const fgRef = useRef<ForceGraphMethods<GraphNode, GraphLink> | undefined>(undefined);

  useEffect(() => {
    const el = canvasWrapRef.current;
    if (!el) return;
    // Measure synchronously on mount rather than waiting solely on the
    // observer's first async callback, which can be delayed or (in a
    // backgrounded/non-composited tab) never fire at all.
    const rect = el.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) setDimensions({ width: rect.width, height: rect.height });
    const observer = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      setDimensions({ width, height });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getGraph(filters);
      const unresolvedSources = new Set(
        data.edges.filter((e) => e.status === "unresolved").map((e) => e.source),
      );
      setNodes(
        data.nodes.map((n) => ({
          id: n.id,
          title: n.title,
          node_type: n.node_type,
          created_by: n.created_by,
          color_key: n.color_key,
          hasUnresolved: unresolvedSources.has(n.id),
        })),
      );
      // force-graph needs a real target to draw a line — unresolved edges
      // (target: null) are represented as the hasUnresolved ring on the
      // source node above instead of an impossible edge.
      setLinks(
        data.edges
          .filter((e): e is typeof e & { target: string } => e.target !== null)
          .map((e) => ({ source: e.source, target: e.target, link_type: e.link_type, status: e.status })),
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [filters]);

  useEffect(() => {
    load();
  }, [load]);

  const graphData = useMemo(() => ({ nodes, links }), [nodes, links]);

  const setFilter = useCallback(<K extends keyof GraphFilters>(key: K, value: GraphFilters[K]) => {
    setFilters((prev) => ({ ...prev, [key]: value || undefined }));
  }, []);

  return (
    <div className="graph-view">
      <div className="graph-filters">
        <select
          value={filters.node_type ?? ""}
          onChange={(e) => setFilter("node_type", e.target.value as NodeType)}
        >
          <option value="">All types</option>
          {NODE_TYPES.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>

        <select value={filters.actor ?? ""} onChange={(e) => setFilter("actor", e.target.value as ActorKind)}>
          <option value="">All authors</option>
          {ACTORS.map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>

        <input
          type="text"
          placeholder="Tag"
          value={filters.tag ?? ""}
          onChange={(e) => setFilter("tag", e.target.value)}
        />

        <label>
          <input
            type="checkbox"
            checked={filters.hide_daily ?? false}
            onChange={(e) => setFilter("hide_daily", e.target.checked)}
          />
          Hide journal
        </label>
        <label>
          <input
            type="checkbox"
            checked={filters.unresolved_only ?? false}
            onChange={(e) => setFilter("unresolved_only", e.target.checked)}
          />
          Unresolved links only
        </label>
        <label>
          <input
            type="checkbox"
            checked={filters.ai_written_only ?? false}
            onChange={(e) => setFilter("ai_written_only", e.target.checked)}
          />
          AI-written only
        </label>
        <label>
          <input
            type="checkbox"
            checked={filters.pending_review_only ?? false}
            onChange={(e) => setFilter("pending_review_only", e.target.checked)}
          />
          Pending review only
        </label>

        <span className="graph-stats">
          {loading ? "Loading…" : `${nodes.length} nodes, ${links.length} edges`}
        </span>
      </div>

      {error && <div className="graph-error">Couldn't load graph: {error}</div>}

      <div className="graph-canvas" ref={canvasWrapRef}>
        {dimensions.width > 0 && dimensions.height > 0 && (
        <ForceGraph2D
          ref={fgRef}
          width={dimensions.width}
          height={dimensions.height}
          graphData={graphData}
          onEngineStop={() => fgRef.current?.zoomToFit(400, 40)}
          nodeId="id"
          nodeLabel={(n) => (n as GraphNode).title}
          nodeColor={(n) => ACTOR_COLOR[(n as GraphNode).color_key as ActorKind] ?? ACTOR_COLOR.user}
          nodeCanvasObjectMode={() => "after"}
          nodeCanvasObject={(n, ctx) => {
            const node = n as GraphNode;
            if (!node.hasUnresolved || node.x === undefined || node.y === undefined) return;
            ctx.beginPath();
            ctx.arc(node.x, node.y, 6, 0, 2 * Math.PI);
            ctx.strokeStyle = "#e05252";
            ctx.lineWidth = 1.5;
            ctx.stroke();
          }}
          linkColor={(l) => ((l as GraphLink).status === "ambiguous" ? "#e0a852" : "#8888")}
          linkDirectionalArrowLength={4}
          linkDirectionalArrowRelPos={1}
          onNodeClick={(n) => onSelect((n as GraphNode).id)}
        />
        )}
      </div>
    </div>
  );
}
