import type { ActorKind, LinkType, LinkStatus, NodeType } from "./node";

export interface GraphNodeDto {
  id: string;
  title: string;
  node_type: NodeType;
  created_by: ActorKind;
  color_key: string;
}

export interface GraphEdgeDto {
  source: string;
  target: string | null;
  link_type: LinkType;
  status: LinkStatus;
}

export interface GraphDto {
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
}

export interface GraphFilters {
  node_type?: string;
  actor?: ActorKind;
  updated_after?: string;
  updated_before?: string;
  tag?: string;
  hide_daily?: boolean;
  unresolved_only?: boolean;
  ai_written_only?: boolean;
  pending_review_only?: boolean;
}
