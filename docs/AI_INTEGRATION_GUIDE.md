# noteapp — Integration Guide for AI Agents

This document is written to be handed to an AI agent (an LLM with tool/HTTP
access) so it can connect to noteapp, understand the data model, migrate
content in from an Obsidian-style markdown vault, and use the system day to
day (reading notes, searching, and writing to the governed AI memory system).

noteapp is a personal, local-first notes vault: SQLite is the source of
truth, a Rust/axum HTTP API sits in front of it, and every write — whether
from the human's desktop app or from an AI agent — goes through the same
API and the same audit log. There is no separate "AI path" that bypasses
review; sensitive or inferred writes are gated behind a propose/approve
queue (see §6).

## 0. Vault policy: this is a cutover, not a dual-write

**As of 2026-08-05, noteapp is the sole source of truth for new notes and
memory. The Obsidian vault at `/opt/data/obsidian` is frozen as a read-only
historical reference — do not write new content to it.**

This matters because the import direction is one-way: `migration/migrate_vault.py`
reads Obsidian markdown and writes into noteapp, but nothing writes the other
way. If an agent kept writing to both, they'd silently fork — content written
to the old vault would never show up in noteapp's search/backlinks/memory,
and vice versa. So the rule is simple: **all new writes — notes, memory,
everything — go through the noteapp API from now on.** Reading the old vault
for historical context is fine; writing to it is not.

(As of this writing, the vault's own `obsidian-headless` continuous-sync
service is also non-functional — its Obsidian Sync subscription expired, so
it isn't pulling in outside edits either. The vault is not just policy-frozen,
it's actually static right now. That's a separate, unrelated issue from this
cutover decision if it ever needs fixing.)

---

## 1. Connecting

**Base URL**: `http://100.126.77.23:47823` (Tailscale) or `http://127.0.0.1:47823`
if you're running on the same host/VM as the container (`kai@192.168.26.105`,
container name `noteapp`).

**Auth**: every request needs `Authorization: Bearer <token>`. There is no
session/cookie auth and no CORS restriction that matters for a server-to-server
caller (the CORS allow-list only exists to let the desktop app's own webview
call the API from a browser context — it doesn't affect `curl`/`requests`/etc).

Three persistent tokens exist, each with a fixed scope set. They were printed
once to the container's logs on first boot (`docker logs noteapp`) and are
otherwise unrecoverable — if lost, the token store file has to be deleted and
the container restarted to mint new ones (which invalidates the old ones).

| Token label | Actor kind | Scopes | Intended caller |
|---|---|---|---|
| `desktop` | `user` | `read, write_notes, write_memory, admin, backup, export, maintenance` | the human's desktop app — full access, "is" the human |
| `agent` | `ai` | `read, write_notes, write_memory` | **an AI agent — use this one** |
| `system` | `system` | `read, write_notes, write_memory, backup, export, maintenance` | offline/ops tooling (migration scripts, cron backups) — not for interactive AI use |

**An AI agent should use the `agent` token**, not `desktop` or `system`. It
deliberately cannot back up, restore, export, or run maintenance — and, more
importantly, it's what makes the server *know* the caller is an AI, which is
what triggers the write-governance rules in §6. Using `system` or `desktop`
for AI traffic would silently bypass those safety rails.

```bash
export NOTEAPP_API_URL=http://100.126.77.23:47823
export NOTEAPP_AGENT_TOKEN=<the 'agent' token from `docker logs noteapp`>

curl -s -H "Authorization: Bearer $NOTEAPP_AGENT_TOKEN" "$NOTEAPP_API_URL/nodes?limit=5"
```

A ready-made CLI wrapper already exists at [`cli/vault_api.py`](../cli/vault_api.py)
(`pip install click requests`, reads the same two env vars) — prefer it over
hand-rolling HTTP calls where it already covers what you need (`context`,
`note get/search/create/update/append`, `review list/propose`).

---

## 2. Core data model

Everything lives in one table, **`nodes`** — there's no separate "folder"
concept; folders are a purely cosmetic grouping the client derives from
`node_type` (or Obsidian's original directory structure, preserved as a
`migration_source_path` property — see §5). A node has:

| Field | Notes |
|---|---|
| `id` | UUID, server-generated, immutable |
| `title` | human-readable, can be renamed |
| `title_normalized` | lowercased/trimmed/whitespace-collapsed, recomputed on every rename — this is one of the two things wikilinks resolve against |
| `slug` | derived from title at creation, **stable across renames** (so export paths/permalinks don't churn) |
| `vault_code` | optional short code like `WK04`, must be globally unique if set (e.g. from `[WK04]-kai.md`) |
| `content` | the raw markdown body |
| `node_type` | one of `page, wiki, journal, project, index, decision, research` (defaults to `page`; there is no per-type validation beyond this enum — arbitrary source folders that don't match a known type just become `page`) |
| `export_policy` | `export` (default) \| `exclude` \| `redact` |
| `created_by` / `updated_by` | `user` \| `ai` \| `system` |
| `properties` | typed key/value pairs (see below) |
| `links` | this node's *outgoing* wikilinks, parsed from `content` (see §3) |

### Properties

Arbitrary typed metadata per node — `text`, `number`, `bool`, `date`, or
`node_ref` (a reference to another node's id). Sent as an array on create/patch:

```json
{ "key": "migration_source_path", "value_type": "text", "value_text": "Wiki/[WK04]-kai.md" }
```

Only one of `value_text`/`value_number`/`value_bool`/`value_date`/`value_node_id`
should be set, matching `value_type`. Properties are upserted by `(node_id, key)`
— posting the same key again overwrites it, it doesn't duplicate.

### Wikilinks and aliases — read this before writing an importer

This is the single most important gotcha in the whole system, and it's the
exact bug this vault's own migration hit.

noteapp parses `[[Title]]`, `[[Title|Display]]`, `[[Title#Heading]]`, and
`![[Embed]]` out of `content` on every create/update/append, and tries to
resolve each link's target against:

1. `nodes.title_normalized` (normalized form of the human-readable `title`), then
2. `aliases.normalized_alias` (normalized form of any alias explicitly attached to a node)

**Obsidian wikilinks reference the raw filename**, e.g. `[[[WK05]-kai-preferences]]`
or `[[Infrastructure/[IN02]-model-routing]]` — not a clean human-readable title
like "Kai Preferences". If you import notes and only set a nice `title`, every
link in the imported content will resolve to nothing (`status: "unresolved"`,
`target_node_id: null`), because nothing in the system knows the raw filename
was ever a name for that note.

**Fix**: after creating a node, also create an alias for it under the raw
filename form(s) it might be linked by — both the bare stem (`[WK05]-kai-preferences`)
and, if the vault has folders, the folder-relative path (`Infrastructure/[IN02]-model-routing`):

```
POST /nodes/{id}/aliases
{ "alias": "[WK05]-kai-preferences" }
```

A 409 response means that alias already exists (on this or another node) —
treat it as a no-op, not a fatal error.

**Second gotcha, easy to miss**: link resolution happens *at write time*,
against whatever aliases exist *at that moment*. If note A links to note B,
but note B (and its alias) didn't exist yet when note A was created, note A's
link to B stays unresolved forever — creating B's alias afterward does **not**
retroactively fix A's already-stored `links` rows. If you're bulk-importing a
vault where notes cross-link each other (almost always true), you need this
exact three-pass structure, not one pass:

1. **Create/update every node's title + content.**
2. **Create every node's aliases** (raw filename forms) — do this for *all*
   nodes before moving to step 3, including ones that already existed from a
   previous run.
3. **Re-save every node's content unchanged** (`PATCH /nodes/{id}` with the
   same `content` it already has) — this forces the server to re-parse and
   re-resolve that node's links now that every other node's alias actually
   exists, regardless of what order nodes were created in.

The real, working implementation of this is [`migration/migrate_vault.py`](../migration/migrate_vault.py)
— reuse it directly rather than reimplementing; it already handles frontmatter
parsing, idempotent re-runs (via a `migration_source_path` property lookup),
and malformed-YAML fallback.

### Search

Full-text search over `title`+`content` via SQLite FTS5:

```
GET /search?q=caffeine&node_type=page&limit=20
```

Your query is split on whitespace and each token is phrase-quoted server-side,
so you don't need to (and can't) use FTS5 query syntax — plain natural-language
queries are the intended input.

### Backlinks

```
GET /nodes/{id}/backlinks
```

Returns every *other* node that has a `resolved` link pointing at this one —
the reverse of the `links` array embedded in `GET /nodes/{id}`.

---

## 3. Full API reference

All responses are JSON. All write routes (`POST`/`PATCH`/`DELETE`) return
`{ "id": "...", "revision_number": <int|null> }`. Errors return
`{ "error": "<message>" }` with an appropriate HTTP status:

| Status | Meaning |
|---|---|
| 400 | invalid input (bad enum value, empty required field, etc.) |
| 401 | missing/invalid bearer token |
| 403 | token lacks the required scope for this route |
| 404 | entity not found (or soft-deleted) |
| 409 | conflict (duplicate `vault_code`, duplicate alias, review item already resolved, etc.) |
| 503 | writer queue unavailable (server shutting down) |

### Nodes — `/nodes` (scope: `read` for GET, `write_notes` for everything else)

```
POST   /nodes                      create a node
  body: { title, node_type?, content?, vault_code?, export_policy?, properties?: [...] }
  node_type defaults to "page"; export_policy defaults to "export"

GET    /nodes?node_type=&limit=    list nodes (summary form), newest-updated first
  limit defaults to 50, clamped to [1, 500]
  -> { "items": [{ id, title, node_type, vault_code, updated_at, created_by }] }

GET    /nodes/{id}                 full node, including properties[] and links[]

PATCH  /nodes/{id}                 partial update
  body: { title?, content?, export_policy?, properties?: [...] }
  omitted fields are left unchanged; title_normalized is recomputed if title changes;
  slug never changes. Re-parses links from the new content.

POST   /nodes/{id}/append          append content
  body: { content_to_append }
  joins with a newline if the node already has content; re-parses links.

DELETE /nodes/{id}                 soft delete (sets deleted_at)
  the node's own outgoing links are removed; other nodes' links still
  pointing at it are left as unresolved-looking dangling references,
  not actively cleaned up.

GET    /nodes/{id}/backlinks       -> { "items": [{ source_node_id, source_title, display_text, link_type }] }

POST   /nodes/{id}/aliases         body: { "alias": "..." }  (409 if already in use anywhere)
GET    /nodes/{id}/aliases         -> { "items": [{ id, node_id, alias, normalized_alias, created_by, created_at }] }
DELETE /nodes/{id}/aliases/{alias_id}
```

### Search — `/search` (scope: `read`)

```
GET /search?q=<query>&node_type=&limit=    limit defaults to 20, clamped to [1, 100]
  -> { "items": [{ id, title, node_type, snippet }] }
```

### Memory — `/memory` (scope: `read` for GET, `write_memory` for writes)

This is the AI-facing memory system — see §6 for the governance model before
writing to it.

```
GET /memory/context?budget_hot=2200&budget_profile=1375
  -> compiled, budgeted, conflict-checked context — the actual thing to load
     at the start of an AI session. See §6 for the response shape.

POST   /memory/hot            create a hot_memory entry directly
GET    /memory/hot            list all non-deleted hot_memory rows
PATCH  /memory/hot/{id}
DELETE /memory/hot/{id}

POST   /memory/profile        create a user_profile entry directly
GET    /memory/profile
PATCH  /memory/profile/{id}
DELETE /memory/profile/{id}
```

`hot_memory` create body:
```json
{
  "key": "preferred_language", "value": "Rust for backend work",
  "category": "preference",        // one of: hard_constraint, preference, identity,
                                    // environment, routing_rule, project_pointer,
                                    // tool_quirk, domain_model, temporary_context
  "scope": "general",              // free text, defaults to "general"
  "priority": 50,                  // 0-100ish, defaults to 50
  "pinned": false,
  "sensitivity": "normal",         // normal | sensitive | secret — see §6
  "source_node_id": null,
  "source_type": "explicit_user_statement",  // see §6 for the full enum + why it matters
  "confidence": "high",            // high | medium | low
  "expires_at": null
}
```
`user_profile` is the same shape plus an optional `source_quote`, and
`category` is free text (no enum) rather than the fixed `hot_memory` list.

### Review queue — `/review` (scope: `write_memory` for everything)

```
POST /review              propose a change (AI-only — see §6)
GET  /review?status=       list proposals, optionally filtered
POST /review/{id}/approve  human/system only
POST /review/{id}/reject   body: { "reason"?: "..." }, human/system only
POST /review/{id}/apply    human/system only — actually performs the write
```

`propose` body:
```json
{
  "proposed_action": "create",              // create | update | delete
  "entity_type": "hot_memory",              // node | hot_memory | user_profile
  "entity_id": null,                        // required for update/delete
  "proposed_diff_json": { "...": "..." },   // shape matches that entity's own create/patch body
  "reason": "user mentioned this twice in one session",
  "confidence": "medium"
}
```

### Changelog — `/changelog` (scope: `read`)

```
GET /changelog?actor=ai&limit=100    limit clamped to [1, 1000]
```
Every write anywhere in the system (node, memory, review resolution) produces
a row here — this is the full audit trail, and it's how the desktop app's
"AI Activity Feed" is built. Filter by `actor` (`user`/`ai`/`system`) to see
just what one caller kind has done.

### Backup / Export (scope: `backup` / `export` — the `agent` token does **not**
have these; use `system` or `desktop` if you genuinely need them)

```
GET  /backup            list snapshots
POST /backup             create one now
POST /backup/{id}/restore   stages a restore, applied on next clean server restart
POST /export              one-way markdown export to the server's export dir
```

---

## 4. What the `agent` token can and can't do

Quick summary, since it's easy to assume "AI token" means "full access":

- ✅ read anything (`GET /nodes`, `/search`, `/memory/*`, `/changelog`, `/review`)
- ✅ create/update/append/delete nodes and aliases directly
- ✅ create/update/delete `hot_memory`/`user_profile` entries directly — **but only**
  `sensitivity: "normal"` and `source_type` other than `"ai_inference"` (see §6)
- ✅ propose review items (`POST /review`)
- ❌ approve/reject/apply review items (`require_non_ai` guard — 400 if attempted)
- ❌ backup, restore, export, maintenance routes (403 — wrong scope)

---

## 5. Migrating content from an Obsidian vault

The full working script is [`migration/migrate_vault.py`](../migration/migrate_vault.py).
Run it directly rather than reimplementing the wikilink/alias logic:

```bash
pip install -r migration/requirements.txt   # click, requests, python-frontmatter
python migration/migrate_vault.py \
  --vault-root /path/to/vault \
  --api-url http://127.0.0.1:47823 \
  --token <a 'system'-scoped token>          # use system, not agent, for bulk migration
  # add --dry-run first to preview, --update-existing to overwrite already-imported content
```

What it does, end to end:

1. Walks the vault for `*.md` files (skipping `.git`/`.obsidian`/etc.), parses
   YAML frontmatter (falling back to treating the whole file as plain content
   if the frontmatter is malformed — don't let one bad file kill the run).
2. Derives `title` from frontmatter `title:` if present, else from the
   filename stem with the `[CODE]-` prefix stripped and dashes turned to
   spaces. Derives `node_type` from the top-level folder name via a small
   lookup table (`Wiki→wiki, Projects→project, Daily→journal, Decisions→decision,
   Research→research`, anything else → `page`). Extracts `vault_code` from a
   `[CODE]-name.md` filename pattern if present.
3. Creates or updates each node, tagging it with a `migration_source_path`
   property (the original relative path) — this is the idempotency key: a
   second run skips content writes for files already imported, unless
   `--update-existing` is passed.
4. **Creates aliases for every node** under both filename-stem and
   folder-relative-path forms.
5. **Re-saves every node's content unchanged**, forcing link re-resolution
   now that every alias exists.

Steps 4–5 always run, even for files skipped in step 3 — so re-running this
script safely backfills aliases/link-resolution onto already-imported notes
too, which is exactly what was needed the first time this ran against the
real vault (see the "one real bug" note below).

**Known real bug this hit, worth internalizing**: the first run of this
migration imported all 36 notes successfully but left every single wikilink
unresolved, because step 4/5 didn't originally exist — only after adding
them did 73 of 78 links resolve. The remaining 5 were confirmed to be
genuine pre-existing typos/placeholder text in the source vault itself
(e.g. a literal `[[Note Name]]` placeholder in a README), not a migration
bug. If you see unresolved links after an import, suspect the alias step
before suspecting the parser.

---

## 6. The AI memory system — governance model

There are two memory tables, both structurally similar:

- **`hot_memory`**: operational facts an AI needs loaded into context regularly
  (preferences, hard constraints, routing rules, tool quirks...).
- **`user_profile`**: longer-lived facts about the human specifically, with
  an optional `source_quote` capturing what they actually said.

Both are governed the same way. The core rule (`guard_direct_write` in the
server code) is:

> An AI actor can write directly **only** when `sensitivity == "normal"` AND
> `source_type != "ai_inference"`. Anything else — a `sensitive`/`secret`
> entry, or anything the AI itself inferred rather than was told outright —
> must go through `POST /review` instead of a direct `POST /memory/hot` or
> `POST /memory/profile`.

In practice: if the user explicitly told you something plain ("I use Rust for
backend work"), write it directly with `source_type: "explicit_user_statement"`,
`sensitivity: "normal"`. If you're inferring something from behavior rather
than being told, or the fact feels sensitive, **propose it** instead and let a
human approve it. A human (`desktop`/`system` token) can always write directly
regardless of sensitivity/source_type — approving *is* the review the AI path
would otherwise need.

### Reading compiled context

`GET /memory/context` is what an agent should call at the start of a session,
not raw `GET /memory/hot` / `/memory/profile` — it does real work:

1. Gathers every `status='approved'`, non-deleted, non-expired entry from
   both tables.
2. Sorts deterministically by: `pinned` desc → category safety rank → `priority`
   desc → source-type trust rank → `confidence` rank → `updated_at` desc → `id`
   asc (final tiebreak) — same order every time, not re-shuffled between calls.
3. Greedily includes entries in that order until `budget_hot`/`budget_profile`
   (character counts, default 2200/1375) would be exceeded; the rest are
   reported as `excluded` with `reason: "budget"`.
4. Flags `warnings` when two approved entries share the same `(category, key)`
   but disagree in value and at least one is high-priority (≥70) — a signal,
   not an automatic resolution.
5. If a table's *included* usage exceeds 80% of its budget, runs an eviction
   pass automatically: the longest, least-recently-included, unpinned,
   non-`hard_constraint` entries get their `value` replaced with a short
   `See [[key]] for full detail.` pointer, with the original full text moved
   into a real new `nodes` row so nothing is actually lost — then recompiles
   and returns the post-eviction result. Pinned and `hard_constraint` entries
   are never evicted.

Response shape:
```json
{
  "compiled_hot_memory": "- [key] value\n- [key2] value2...",
  "compiled_user_profile": "...",
  "included": [{ "id", "table", "key", "source_node_id" }],
  "excluded": [{ "id", "table", "key", "reason": "budget" }],
  "budget_usage": { "hot_used", "hot_budget", "profile_used", "profile_budget" },
  "warnings": [{ "type": "conflict", "category", "key", "entry_ids": [...] }],
  "compiler_version": "1.0.0"
}
```

### Review queue lifecycle

`pending → approved → applied`, or `pending → rejected`. Only an AI-scoped
token can `propose`; only a non-AI token can `approve`/`reject`/`apply` — an
AI cannot resolve its own proposal, by design. `apply` re-validates that the
proposal's target still exists *inside* the same transaction as the write
(closing the gap where something could be deleted between approval and
apply), then performs the actual `hot_memory`/`user_profile`/`node` mutation
and records it in `/changelog` with `applied_changelog_id` set on the review
row — the queue row itself is never the durable fact, it's a pointer to the
changelog entry that made it durable.

---

## 7. Practical notes / gotchas

- **`slug` never changes**, even on rename — don't rely on it as a display
  title, and don't expect export paths to move when a note is retitled.
- **`vault_code` must be globally unique** if you set one — reusing one from
  another node returns 409.
- **Deleting a node is soft** (`deleted_at` set) — its own outgoing links are
  removed, but other notes that link *to* it keep their link row, which will
  just look permanently unresolved. There's no automatic backlink cleanup.
- **FTS search sanitizes your query** by phrase-quoting each token — you
  cannot use FTS5 operators (`NEAR`, column filters, boolean `OR`); send
  plain natural-language queries.
- **`GET /nodes` (list) returns summaries only** — no `content`, no `links`,
  no `properties`. Fetch `GET /nodes/{id}` for the full record.
- Property writes are **upserts by `(node_id, key)`** — posting a property
  with a key that already exists on that node overwrites it in place, it
  doesn't append a duplicate.
