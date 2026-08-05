-- schema.sql (v3)
--
-- Changes from v2, per architectural review:
--   - CHECK constraints on all enum-like fields (SQLite won't enforce comments)
--   - nodes: title_normalized (link resolution) + vault_code (stable human-readable
--     ID, e.g. WK04) kept distinct from slug (export path)
--   - nodes: export_policy as a real column, not a metadata_json convention
--   - hot_memory / user_profile: scope, deleted_at, deprecated_at, replaced_by_id
--   - user_profile: source_quote (preserve exact user phrasing behind a fact)
--   - ai_review_queue: explicit apply lifecycle (pending -> approved -> applied
--     | rejected), proposed_by preserved separately from who applies it
--   - indexes added for every common query path
--
-- Deliberately deferred to v1.1+ (do not build prematurely):
--   - conflicts_with field / full conflict-resolution UI (v1 ships compiler
--     warnings only, based on same category+key with differing high-priority
--     entries — no dedicated table yet)
--   - changelog hash chain (previous_hash / entry_hash)
--   - compiled_context_runs table (debugging aid, not needed to ship)
--   - scope as JSON (start with a flat TEXT scope column; upgrade only if
--     flat strings prove insufficient)

-- journal_mode/foreign_keys/synchronous/busy_timeout are applied by the app itself
-- (db::pool::open_write_connection / build_read_pool) on every connection open,
-- not here: refinery runs each migration inside a transaction, and SQLite
-- rejects `PRAGMA synchronous` ("Safety level may not be changed inside a
-- transaction") in that context — journal_mode/foreign_keys would silently
-- no-op the same way, so none of the four belong in a migration file.

-- ============================================================
-- PHASE 0 — core, load-bearing, do not retrofit later
-- ============================================================

CREATE TABLE IF NOT EXISTS nodes (
    id               TEXT PRIMARY KEY,          -- uuid, internal identity
    title            TEXT NOT NULL,
    title_normalized TEXT NOT NULL,              -- lowercased/trimmed, used for wikilink resolution
    slug             TEXT NOT NULL UNIQUE,        -- url/path-safe, used for export filenames
    vault_code       TEXT UNIQUE,                 -- stable human-readable ID, e.g. 'WK04', 'PJ02'
                                                    -- (nullable for casual notes, expected for
                                                    -- wiki/project/index-type nodes)
    content          TEXT NOT NULL DEFAULT '',
    node_type        TEXT NOT NULL DEFAULT 'page'
        CHECK (node_type IN ('page','wiki','journal','project','index','decision','research')),
    export_policy    TEXT NOT NULL DEFAULT 'export'
        CHECK (export_policy IN ('export','exclude','redact')),
    created_by       TEXT NOT NULL CHECK (created_by IN ('user','ai','system')),
    updated_by       TEXT NOT NULL CHECK (updated_by IN ('user','ai','system')),
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    deleted_at       TEXT,                        -- soft delete; keep history intact
    metadata_json    TEXT                         -- freeform extension point only; nothing
                                                    -- load-bearing should live only here
);

CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(node_type);
CREATE INDEX IF NOT EXISTS idx_nodes_updated_at ON nodes(updated_at);
CREATE INDEX IF NOT EXISTS idx_nodes_created_by ON nodes(created_by);
CREATE INDEX IF NOT EXISTS idx_nodes_slug ON nodes(slug);
CREATE INDEX IF NOT EXISTS idx_nodes_deleted_at ON nodes(deleted_at);
CREATE INDEX IF NOT EXISTS idx_nodes_title_normalized ON nodes(title_normalized);

CREATE TABLE IF NOT EXISTS properties (
    id             TEXT PRIMARY KEY,
    node_id        TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    key            TEXT NOT NULL,
    value_type     TEXT NOT NULL
        CHECK (value_type IN ('text','number','bool','date','node_ref')),
    value_text     TEXT,
    value_number   REAL,
    value_bool     INTEGER,
    value_date     TEXT,
    value_node_id  TEXT REFERENCES nodes(id),
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    UNIQUE(node_id, key)
);

CREATE INDEX IF NOT EXISTS idx_properties_node_id ON properties(node_id);
CREATE INDEX IF NOT EXISTS idx_properties_key ON properties(key);
CREATE INDEX IF NOT EXISTS idx_properties_value_node_id ON properties(value_node_id);

-- Wikilinks. Distinguishes resolved vs unresolved so the UI can surface dangling
-- links and the AI can help clean them up.
CREATE TABLE IF NOT EXISTS links (
    id              TEXT PRIMARY KEY,
    source_node_id  TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    target_node_id  TEXT REFERENCES nodes(id) ON DELETE SET NULL,
    target_raw      TEXT NOT NULL,             -- literal text inside [[ ]] as written
    display_text    TEXT,                      -- alias text, e.g. [[Title|Alias]]
    link_type       TEXT NOT NULL DEFAULT 'wikilink' CHECK (link_type IN ('wikilink','embed')),
    status          TEXT NOT NULL DEFAULT 'unresolved'
        CHECK (status IN ('resolved','unresolved','ambiguous')),
    source_start    INTEGER,                   -- char offset in source content, for re-parsing
    source_end      INTEGER,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_node_id);
CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_node_id);
CREATE INDEX IF NOT EXISTS idx_links_status ON links(status);

-- Rich, append-only audit log. Every mutation anywhere in the system writes here
-- in the same transaction as the mutation itself. No update/delete endpoint should
-- ever be exposed for this table.
CREATE TABLE IF NOT EXISTS changelog (
    id                TEXT PRIMARY KEY,
    timestamp         TEXT NOT NULL,
    actor             TEXT NOT NULL CHECK (actor IN ('user','ai','system')),
    action            TEXT NOT NULL CHECK (action IN ('create','update','append','delete')),
    entity_type       TEXT NOT NULL,           -- 'node' | 'property' | 'hot_memory' | 'user_profile' | ...
    entity_id         TEXT NOT NULL,
    before_hash       TEXT,
    after_hash        TEXT,
    diff_json         TEXT,                    -- unified diff for text, JSON before/after for properties
    reason            TEXT,                    -- why the write happened, esp. important for AI writes
    source_session_id TEXT,
    source_task_id    TEXT,
    request_id        TEXT NOT NULL,
    compiler_version  TEXT                     -- set when the write originates from a memory compile
);

CREATE INDEX IF NOT EXISTS idx_changelog_entity ON changelog(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_changelog_actor ON changelog(actor);
CREATE INDEX IF NOT EXISTS idx_changelog_timestamp ON changelog(timestamp);
CREATE INDEX IF NOT EXISTS idx_changelog_request_id ON changelog(request_id);

-- Full historical snapshots, separate from the changelog's diffs — makes rollback
-- and "show me this note on date X" trivial instead of replaying diffs.
CREATE TABLE IF NOT EXISTS node_revisions (
    id                       TEXT PRIMARY KEY,
    node_id                  TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    revision_number          INTEGER NOT NULL,
    title                    TEXT NOT NULL,
    content                  TEXT NOT NULL,
    properties_snapshot_json TEXT,
    content_hash             TEXT NOT NULL,
    changelog_id             TEXT REFERENCES changelog(id),
    created_at               TEXT NOT NULL,
    created_by               TEXT NOT NULL CHECK (created_by IN ('user','ai','system')),
    UNIQUE(node_id, revision_number)
);

CREATE INDEX IF NOT EXISTS idx_node_revisions_node_id ON node_revisions(node_id);

-- Full-text search (external-content FTS5). `nodes` retains its implicit SQLite
-- rowid (no WITHOUT ROWID), which this table maps to internally — never expose
-- rowid as note identity outside this file; external identity is always nodes.id.
-- Keep this table in sync via triggers or backend writes in the same transaction
-- as nodes writes; do not leave the sync strategy ambiguous.
CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
    title, content, content='nodes', content_rowid='rowid'
);

-- ============================================================
-- PHASE 3 — memory system + AI governance (additive, not schema-breaking)
--
-- Canonical semantics: canonical truth lives in `nodes`/`properties` and
-- approved `user_profile` facts. `hot_memory` and `user_profile` store
-- governed candidate/approved entries with full provenance — they are NOT
-- raw prompt text. The actual AI-loaded context is always the output of the
-- memory compiler (GET /memory/context), which reads these tables plus
-- canonical nodes and assembles the final budgeted text deterministically.
-- Nothing should ever inject these tables' rows directly into a prompt
-- without going through the compiler.
-- ============================================================

-- Resolves name variants ("Kai" / "Karl" / "@Kai-Doh") to a single node, preventing
-- duplicate graph nodes for the same real-world entity. Uses the same
-- normalization function as nodes.title_normalized.
CREATE TABLE IF NOT EXISTS aliases (
    id                TEXT PRIMARY KEY,
    node_id           TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    alias             TEXT NOT NULL,
    normalized_alias  TEXT NOT NULL,
    created_by        TEXT NOT NULL CHECK (created_by IN ('user','ai','system')),
    created_at        TEXT NOT NULL,
    UNIQUE(normalized_alias)
);

CREATE INDEX IF NOT EXISTS idx_aliases_node_id ON aliases(node_id);

-- Governed memory entries — candidate/approved facts with provenance. The
-- compiler selects, ranks, and budgets these into the final injected text;
-- this table is not itself the prompt.
CREATE TABLE IF NOT EXISTS hot_memory (
    id              TEXT PRIMARY KEY,
    key             TEXT NOT NULL,
    value           TEXT NOT NULL,             -- short, pointer-heavy text
    category        TEXT NOT NULL CHECK (category IN
        ('hard_constraint','preference','identity','environment','routing_rule',
         'project_pointer','tool_quirk','domain_model','temporary_context')),
    status          TEXT NOT NULL DEFAULT 'approved'
        CHECK (status IN ('candidate','approved','rejected','deprecated')),
                                                    -- only 'approved' rows are eligible for
                                                    -- compilation; ai_review_queue governs the
                                                    -- transition into 'approved' for sensitive writes
    scope           TEXT NOT NULL DEFAULT 'general', -- e.g. 'general','food_planning',
                                                        -- 'code_output','design_review'
    priority        INTEGER NOT NULL DEFAULT 50,
    pinned          INTEGER NOT NULL DEFAULT 0, -- pinned entries are never auto-evicted
    sensitivity     TEXT NOT NULL DEFAULT 'normal' CHECK (sensitivity IN ('normal','sensitive','secret')),
    source_node_id  TEXT REFERENCES nodes(id),  -- where the full detail lives, if any
    source_type     TEXT NOT NULL DEFAULT 'explicit_user_statement' CHECK (source_type IN
        ('explicit_user_statement','observed_behavior','imported_note','system_config','ai_inference')),
    confidence      TEXT NOT NULL DEFAULT 'high' CHECK (confidence IN ('high','medium','low')),
    char_count      INTEGER NOT NULL,
    created_by      TEXT NOT NULL CHECK (created_by IN ('user','ai','system')),
    updated_by      TEXT NOT NULL CHECK (updated_by IN ('user','ai','system')),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    expires_at      TEXT,
    deprecated_at   TEXT,                       -- no longer active, but kept for history
    replaced_by_id  TEXT REFERENCES hot_memory(id),
    deleted_at      TEXT,
    last_included_at TEXT,                      -- last time the compiler included this entry
    include_count   INTEGER NOT NULL DEFAULT 0   -- batched by system, not updated per-request
);

CREATE INDEX IF NOT EXISTS idx_hot_memory_status ON hot_memory(status);
CREATE INDEX IF NOT EXISTS idx_hot_memory_category ON hot_memory(category);
CREATE INDEX IF NOT EXISTS idx_hot_memory_priority ON hot_memory(priority);
CREATE INDEX IF NOT EXISTS idx_hot_memory_pinned ON hot_memory(pinned);
CREATE INDEX IF NOT EXISTS idx_hot_memory_source_node_id ON hot_memory(source_node_id);
CREATE INDEX IF NOT EXISTS idx_hot_memory_expires_at ON hot_memory(expires_at);
CREATE INDEX IF NOT EXISTS idx_hot_memory_deleted_at ON hot_memory(deleted_at);

-- Same shape as hot_memory, scoped to durable facts about the human user.
-- source_quote preserves exact user phrasing behind a fact, distinct from any
-- AI paraphrase drift in `value`.
CREATE TABLE IF NOT EXISTS user_profile (
    id              TEXT PRIMARY KEY,
    key             TEXT NOT NULL,
    value           TEXT NOT NULL,
    source_quote    TEXT,                       -- exact user wording, when available
    category        TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'approved'
        CHECK (status IN ('candidate','approved','rejected','deprecated')),
                                                    -- only 'approved' rows are eligible for
                                                    -- compilation; ai_review_queue governs the
                                                    -- transition into 'approved' for sensitive writes
    scope           TEXT NOT NULL DEFAULT 'general',
    priority        INTEGER NOT NULL DEFAULT 50,
    pinned          INTEGER NOT NULL DEFAULT 0,
    sensitivity     TEXT NOT NULL DEFAULT 'normal' CHECK (sensitivity IN ('normal','sensitive','secret')),
    source_node_id  TEXT REFERENCES nodes(id),
    source_type     TEXT NOT NULL DEFAULT 'explicit_user_statement' CHECK (source_type IN
        ('explicit_user_statement','observed_behavior','imported_note','system_config','ai_inference')),
    confidence      TEXT NOT NULL DEFAULT 'high' CHECK (confidence IN ('high','medium','low')),
    char_count      INTEGER NOT NULL,
    created_by      TEXT NOT NULL CHECK (created_by IN ('user','ai','system')),
    updated_by      TEXT NOT NULL CHECK (updated_by IN ('user','ai','system')),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    expires_at      TEXT,
    deprecated_at   TEXT,
    replaced_by_id  TEXT REFERENCES user_profile(id),
    deleted_at      TEXT,
    last_included_at TEXT,
    include_count   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_user_profile_status ON user_profile(status);
CREATE INDEX IF NOT EXISTS idx_user_profile_category ON user_profile(category);
CREATE INDEX IF NOT EXISTS idx_user_profile_priority ON user_profile(priority);
CREATE INDEX IF NOT EXISTS idx_user_profile_pinned ON user_profile(pinned);
CREATE INDEX IF NOT EXISTS idx_user_profile_source_node_id ON user_profile(source_node_id);
CREATE INDEX IF NOT EXISTS idx_user_profile_deleted_at ON user_profile(deleted_at);

-- Sensitive or inferred AI writes land here for human approval before they become
-- durable memory/profile truth. Lifecycle: pending -> approved -> applied, or
-- pending -> rejected. "Approved" alone does not mutate anything; only /apply
-- runs the proposed mutation through the standard write-transaction helper,
-- which produces its own changelog row (actor='user' or 'system') while this
-- row's `actor` field preserves that the original proposal came from 'ai'.
CREATE TABLE IF NOT EXISTS ai_review_queue (
    id                 TEXT PRIMARY KEY,
    created_at         TEXT NOT NULL,
    actor              TEXT NOT NULL DEFAULT 'ai' CHECK (actor IN ('ai')), -- proposals are always AI-originated
    proposed_action    TEXT NOT NULL CHECK (proposed_action IN ('create','update','delete')),
    entity_type        TEXT NOT NULL CHECK (entity_type IN ('node','hot_memory','user_profile')),
    entity_id          TEXT,                   -- null if proposing a brand-new entity
    proposed_diff_json TEXT NOT NULL,
    reason             TEXT,
    confidence         TEXT CHECK (confidence IN ('high','medium','low')),
    status             TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','approved','rejected','applied')),
    resolved_by        TEXT CHECK (resolved_by IN ('user','system')),
    resolved_at        TEXT,
    applied_changelog_id TEXT REFERENCES changelog(id)
);

CREATE INDEX IF NOT EXISTS idx_review_queue_status ON ai_review_queue(status);

-- Lint/maintenance findings — turns the daily lint pass into queryable data instead
-- of a terminal script.
CREATE TABLE IF NOT EXISTS maintenance_findings (
    id              TEXT PRIMARY KEY,
    finding_type    TEXT NOT NULL,   -- 'orphan_node'|'unresolved_link'|'stale_memory'|
                                       -- 'conflicting_memory'|...
    severity        TEXT NOT NULL CHECK (severity IN ('info','warning','critical')),
    entity_id       TEXT,
    description     TEXT NOT NULL,
    suggested_fix   TEXT,
    status          TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','resolved','ignored')),
    created_at      TEXT NOT NULL,
    resolved_at     TEXT
);
