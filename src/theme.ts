import type { ActorKind } from "./types/node";

// Single source of truth for actor colors — used by both AuthorBadge (inline
// style on a DOM element) and GraphView (canvas rendering via
// react-force-graph-2d), which needs literal color strings rather than CSS
// custom properties since canvas contexts can't resolve `var(...)` directly.
// Values match the Nocturne theme's --color-neutral-300/--color-accent/--color-neutral-600.
export const ACTOR_COLOR: Record<ActorKind, string> = {
  user: "#cfd3e5",
  ai: "#9184d9",
  system: "#75798c",
};
