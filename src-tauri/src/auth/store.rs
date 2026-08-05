use std::collections::HashMap;
use std::path::Path;

use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::auth::scope::{Scope, ScopeSet};
use crate::db::writer::ActorKind;

#[derive(Debug, Serialize, Deserialize)]
struct PersistedToken {
    token_hash: String,
    label: String,
    kind: String, // "user" | "ai" | "system"
    scopes: String, // csv, e.g. "read,write_notes"
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PersistedStore {
    tokens: Vec<PersistedToken>,
}

/// The authenticated caller for one request. Cheap to clone — carried as an axum
/// request extension from `auth::middleware::auth_middleware` into every handler.
#[derive(Clone, Debug)]
pub struct AuthedActor {
    pub label: String,
    pub kind: ActorKind,
    pub scopes: ScopeSet,
}

/// Returned by `AuthedActor::require` when the caller's token lacks a needed
/// scope. Kept independent of `api::error::ApiError` (which converts it via
/// `From`) so the auth module doesn't need to depend on the API layer.
#[derive(Debug)]
pub struct ScopeError(pub String);

impl AuthedActor {
    pub fn require(&self, scope: Scope) -> Result<(), ScopeError> {
        if self.scopes.contains(scope) {
            Ok(())
        } else {
            Err(ScopeError(format!(
                "token '{}' is missing required scope: {}",
                self.label,
                scope.as_str()
            )))
        }
    }
}

pub struct TokenCache {
    by_hash: HashMap<String, AuthedActor>,
}

impl TokenCache {
    pub fn lookup(&self, plaintext_token: &str) -> Option<&AuthedActor> {
        self.by_hash.get(&hash_token(plaintext_token))
    }
}

fn hash_token(plaintext: &str) -> String {
    blake3::hash(plaintext.as_bytes()).to_hex().to_string()
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn kind_from_str(s: &str) -> ActorKind {
    match s {
        "ai" => ActorKind::Ai,
        "system" => ActorKind::System,
        _ => ActorKind::User,
    }
}

/// (label, actor kind, scopes) for every token this server issues, generated
/// once and persisted (hashed) so a human copies each one into its consumer's
/// config exactly once: `desktop` for the thin-client desktop app (full human
/// scope — the desktop app *is* the human, once they've pasted this in),
/// `agent` for an AI caller (e.g. the CLI bridge) — deliberately no
/// admin/backup/export/maintenance, an AI agent has no business triggering
/// those — `system` for offline/ops tooling (the migration script, a
/// scheduled backup cron job, scripted maintenance), which does need that
/// operational surface.
///
/// All three are persistent now — there is no more ephemeral/in-process `ui`
/// token. That model only worked when the frontend and backend were the same
/// process on the same machine; once the backend runs in a container the
/// desktop app connects to over the network, its token has to be a real
/// credential the user configures once (see `client_config` on the desktop
/// side), same as any other external caller.
const TOKEN_DEFS: [(&str, &str, &str); 3] = [
    ("desktop", "user", "read,write_notes,write_memory,admin,backup,export,maintenance"),
    ("agent", "ai", "read,write_notes,write_memory"),
    ("system", "system", "read,write_notes,write_memory,backup,export,maintenance"),
];

/// Loads the persisted tokens from the local JSON store, or generates and
/// persists new ones on first run — printing each plaintext token exactly
/// once (to stderr/`docker logs`). Not a database table: this is
/// infrastructure, kept out of the approved notes/memory schema.
pub fn load_or_init(path: &Path) -> TokenCache {
    if path.exists() {
        let data = std::fs::read_to_string(path).expect("failed to read token store");
        let persisted: PersistedStore = serde_json::from_str(&data).expect("corrupt token store");
        let mut by_hash = HashMap::new();
        for t in persisted.tokens {
            by_hash.insert(
                t.token_hash,
                AuthedActor {
                    label: t.label,
                    kind: kind_from_str(&t.kind),
                    scopes: ScopeSet::from_csv(&t.scopes),
                },
            );
        }
        TokenCache { by_hash }
    } else {
        let mut persisted = PersistedStore::default();
        let mut by_hash = HashMap::new();
        // stderr, not stdout: stdout is block-buffered once piped/redirected (unlike
        // tracing's writer, which flushes per event), so println! here could sit
        // unflushed for the life of the process — stderr is always unbuffered.
        eprintln!("== noteapp: generated local API tokens (each shown once — copy them now) ==");
        for (label, kind, scopes_csv) in TOKEN_DEFS {
            let plaintext = generate_token();
            eprintln!("  {label:<8} [{kind:<6}]: {plaintext}");
            let hash = hash_token(&plaintext);
            persisted.tokens.push(PersistedToken {
                token_hash: hash.clone(),
                label: label.to_string(),
                kind: kind.to_string(),
                scopes: scopes_csv.to_string(),
            });
            by_hash.insert(
                hash,
                AuthedActor {
                    label: label.to_string(),
                    kind: kind_from_str(kind),
                    scopes: ScopeSet::from_csv(scopes_csv),
                },
            );
        }
        eprintln!("== plaintext tokens are not stored anywhere and will not be shown again ==");
        let json = serde_json::to_string_pretty(&persisted).expect("serialize token store");
        std::fs::write(path, json).expect("failed to write token store");
        TokenCache { by_hash }
    }
}
