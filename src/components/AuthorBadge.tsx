import type { ActorKind } from "../types/node";

// Same palette as GraphView's color_key coloring, so "who wrote this" reads
// consistently whether you're looking at the list or the graph.
const COLOR_BY_ACTOR: Record<ActorKind, string> = {
  user: "#4f8cff",
  ai: "#a970ff",
  system: "#6b7280",
};

const LABEL_BY_ACTOR: Record<ActorKind, string> = {
  user: "You",
  ai: "AI",
  // "system" == notes written by tooling acting on your behalf (mainly the
  // one-time Obsidian vault import) rather than live use by you or Baymax —
  // "Imported" says what that actually means; "System" didn't.
  system: "Imported",
};

const TITLE_BY_ACTOR: Record<ActorKind, string> = {
  user: "Written by you",
  ai: "Written by Baymax (AI)",
  system: "Imported by tooling (e.g. the Obsidian vault migration), not written live by you or Baymax",
};

interface AuthorBadgeProps {
  actor: ActorKind;
}

export function AuthorBadge({ actor }: AuthorBadgeProps) {
  return (
    <span
      className="author-badge"
      style={{ color: COLOR_BY_ACTOR[actor], borderColor: COLOR_BY_ACTOR[actor] }}
      title={TITLE_BY_ACTOR[actor]}
    >
      {LABEL_BY_ACTOR[actor]}
    </span>
  );
}
