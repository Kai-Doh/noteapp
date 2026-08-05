export type NodeType =
  | "page"
  | "wiki"
  | "journal"
  | "project"
  | "index"
  | "decision"
  | "research";

export type ExportPolicy = "export" | "exclude" | "redact";

export type ActorKind = "user" | "ai" | "system";

export type PropertyValueType = "text" | "number" | "bool" | "date" | "node_ref";

export interface PropertyInput {
  key: string;
  value_type: PropertyValueType;
  value_text?: string | null;
  value_number?: number | null;
  value_bool?: boolean | null;
  value_date?: string | null;
  value_node_id?: string | null;
}

export type LinkStatus = "resolved" | "unresolved" | "ambiguous";
export type LinkType = "wikilink" | "embed";

export interface OutgoingLinkDto {
  id: string;
  target_node_id: string | null;
  target_raw: string;
  display_text: string | null;
  link_type: LinkType;
  status: LinkStatus;
}

export interface BacklinkDto {
  source_node_id: string;
  source_title: string;
  display_text: string | null;
  link_type: LinkType;
}

export interface NodeDto {
  id: string;
  title: string;
  title_normalized: string;
  slug: string;
  vault_code: string | null;
  content: string;
  node_type: NodeType;
  export_policy: ExportPolicy;
  created_by: ActorKind;
  updated_by: ActorKind;
  created_at: string;
  updated_at: string;
  properties: PropertyInput[];
  links: OutgoingLinkDto[];
}

export interface NodeSummaryDto {
  id: string;
  title: string;
  node_type: NodeType;
  vault_code: string | null;
  updated_at: string;
  created_by: ActorKind;
  folder: string | null;
}

export interface WriteResultDto {
  id: string;
  revision_number: number | null;
}

export interface SearchHitDto {
  id: string;
  title: string;
  node_type: NodeType;
  snippet: string;
}

export interface AliasDto {
  id: string;
  node_id: string;
  alias: string;
  normalized_alias: string;
  created_by: ActorKind;
  created_at: string;
}
