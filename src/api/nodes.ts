import { apiFetch } from "./client";
import type {
  ActorKind,
  AliasDto,
  BacklinkDto,
  NodeDto,
  NodeSummaryDto,
  PropertyInput,
  WriteResultDto,
} from "../types/node";

export interface CreateNodeInput {
  title: string;
  node_type?: string;
  content?: string;
  vault_code?: string | null;
  export_policy?: string | null;
  properties?: PropertyInput[];
}

export interface PatchNodeInput {
  title?: string;
  content?: string;
  export_policy?: string | null;
  properties?: PropertyInput[];
}

export function createNode(input: CreateNodeInput): Promise<WriteResultDto> {
  return apiFetch("/nodes", { method: "POST", body: JSON.stringify(input) });
}

export function patchNode(id: string, input: PatchNodeInput): Promise<WriteResultDto> {
  return apiFetch(`/nodes/${id}`, { method: "PATCH", body: JSON.stringify(input) });
}

export function appendNode(id: string, contentToAppend: string): Promise<WriteResultDto> {
  return apiFetch(`/nodes/${id}/append`, {
    method: "POST",
    body: JSON.stringify({ content_to_append: contentToAppend }),
  });
}

export function getNode(id: string): Promise<NodeDto> {
  return apiFetch(`/nodes/${id}`);
}

export function deleteNode(id: string): Promise<WriteResultDto> {
  return apiFetch(`/nodes/${id}`, { method: "DELETE" });
}

export function listNodes(
  params: { node_type?: string; created_by?: ActorKind; limit?: number } = {},
): Promise<{
  items: NodeSummaryDto[];
}> {
  const q = new URLSearchParams();
  if (params.node_type) q.set("node_type", params.node_type);
  if (params.created_by) q.set("created_by", params.created_by);
  if (params.limit) q.set("limit", String(params.limit));
  const qs = q.toString();
  return apiFetch(`/nodes${qs ? `?${qs}` : ""}`);
}

export function getBacklinks(id: string): Promise<{ items: BacklinkDto[] }> {
  return apiFetch(`/nodes/${id}/backlinks`);
}

export function createAlias(nodeId: string, alias: string): Promise<WriteResultDto> {
  return apiFetch(`/nodes/${nodeId}/aliases`, { method: "POST", body: JSON.stringify({ alias }) });
}

export function listAliases(nodeId: string): Promise<{ items: AliasDto[] }> {
  return apiFetch(`/nodes/${nodeId}/aliases`);
}

export function deleteAlias(nodeId: string, aliasId: string): Promise<WriteResultDto> {
  return apiFetch(`/nodes/${nodeId}/aliases/${aliasId}`, { method: "DELETE" });
}

// Finds a node by exact title match — used by wikilink click-to-navigate,
// which only has display text (a title), not an id, to work from.
export async function findNodeByTitle(title: string): Promise<NodeSummaryDto | null> {
  const normalized = title.trim().toLowerCase();
  const { items } = await listNodes({ limit: 500 });
  return items.find((n) => n.title.trim().toLowerCase() === normalized) ?? null;
}
