import type { ActorKind } from "./node";

export interface ReviewItemDto {
  id: string;
  created_at: string;
  actor: ActorKind;
  proposed_action: "create" | "update" | "delete";
  entity_type: "node" | "hot_memory" | "user_profile";
  entity_id: string | null;
  proposed_diff_json: unknown;
  reason: string | null;
  confidence: "high" | "medium" | "low" | null;
  status: "pending" | "approved" | "rejected" | "applied";
  resolved_by: "user" | "system" | null;
  resolved_at: string | null;
  applied_changelog_id: string | null;
}

export interface ChangelogEntryDto {
  id: string;
  timestamp: string;
  actor: ActorKind;
  action: "create" | "update" | "append" | "delete";
  entity_type: string;
  entity_id: string;
  diff_json: unknown;
  reason: string | null;
  compiler_version: string | null;
}
