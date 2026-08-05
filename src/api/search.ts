import { apiFetch } from "./client";
import type { SearchHitDto } from "../types/node";

export function searchNodes(
  q: string,
  opts: { node_type?: string; limit?: number } = {},
): Promise<{ items: SearchHitDto[] }> {
  const params = new URLSearchParams({ q });
  if (opts.node_type) params.set("node_type", opts.node_type);
  if (opts.limit) params.set("limit", String(opts.limit));
  return apiFetch(`/search?${params.toString()}`);
}
