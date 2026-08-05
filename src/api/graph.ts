import { apiFetch } from "./client";
import type { GraphDto, GraphFilters } from "../types/graph";

export function getGraph(filters: GraphFilters = {}): Promise<GraphDto> {
  const q = new URLSearchParams();
  if (filters.node_type) q.set("node_type", filters.node_type);
  if (filters.actor) q.set("actor", filters.actor);
  if (filters.updated_after) q.set("updated_after", filters.updated_after);
  if (filters.updated_before) q.set("updated_before", filters.updated_before);
  if (filters.tag) q.set("tag", filters.tag);
  if (filters.hide_daily) q.set("hide_daily", "true");
  if (filters.unresolved_only) q.set("unresolved_only", "true");
  if (filters.ai_written_only) q.set("ai_written_only", "true");
  if (filters.pending_review_only) q.set("pending_review_only", "true");
  const qs = q.toString();
  return apiFetch(`/graph${qs ? `?${qs}` : ""}`);
}
