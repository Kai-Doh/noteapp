import type { ActorKind } from "../types/node";
import { ACTOR_COLOR } from "../theme";

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
      style={{ color: ACTOR_COLOR[actor], borderColor: ACTOR_COLOR[actor] }}
      title={TITLE_BY_ACTOR[actor]}
    >
      {LABEL_BY_ACTOR[actor]}
    </span>
  );
}
