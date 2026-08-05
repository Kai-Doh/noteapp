#!/usr/bin/env python3
"""One-time migration from an existing markdown vault into noteapp.

Standalone, rough-and-ready per the plan's own scoping — this is an import,
not a sync. Source markdown files are never modified or deleted; every write
goes through the real local API (not a direct DB write), so it gets the same
wikilink resolution, FTS5 indexing, and changelog attribution as any other
write. Idempotent: re-running skips node creation for files already imported
(tracked via a `migration_source_path` property on each created node) unless
--update-existing is passed — but always re-checks aliases and re-triggers
link resolution, since those depend on *other* files' state too (see below).

Three passes, not one:
  1. create/update each node's title+content
  2. create an alias for every node under both its raw filename stem and its
     vault-relative path (Obsidian wikilinks reference the *filename*, e.g.
     `[[[WK05]-kai-preferences]]` — not the human-readable title this script
     derives, e.g. "Kai Preferences" — so without these aliases nothing
     resolves)
  3. re-PATCH every node with its own unchanged content, forcing the backend
     to re-parse and re-resolve its links now that every other node's alias
     actually exists — pass 1 alone can't do this: a note whose *target*
     doesn't have its alias yet when *this* note gets created would stay
     unresolved forever without this forced second pass, regardless of
     creation order

Usage:
    pip install -r requirements.txt
    python migrate_vault.py --vault-root /path/to/vault --token <system-token>
    python migrate_vault.py --vault-root /path/to/vault --token <system-token> --dry-run
"""

import re
from pathlib import Path

import click
import frontmatter
import requests

# Generic convention from the roadmap. "Fitness" and "Infrastructure" (real
# folders in the vault this was written against) have no dedicated node_type
# in the schema's CHECK constraint (only page/wiki/journal/project/index/
# decision/research) — they fall back to "page" like anything else not in
# this table, same as any future new folder.
FOLDER_TO_NODE_TYPE = {
    "wiki": "wiki",
    "projects": "project",
    "daily": "journal",
    "decisions": "decision",
    "research": "research",
}

VAULT_CODE_RE = re.compile(r"^\[([A-Za-z0-9]+)\]-(.+)$")
SKIP_DIRS = {".git", ".git-corrupted", ".obsidian", ".rag"}


def extract_vault_code(stem: str) -> str | None:
    m = VAULT_CODE_RE.match(stem)
    return m.group(1) if m else None


def derive_title(meta: dict, stem: str) -> str:
    if meta.get("title"):
        return str(meta["title"])
    m = VAULT_CODE_RE.match(stem)
    name = m.group(2) if m else stem
    return name.replace("-", " ").replace("_", " ").strip() or stem


def infer_node_type(rel_path: Path) -> str:
    if len(rel_path.parts) > 1:
        return FOLDER_TO_NODE_TYPE.get(rel_path.parts[0].lower(), "page")
    return "page"


def walk_vault(root: Path):
    for md_file in sorted(root.rglob("*.md")):
        rel = md_file.relative_to(root)
        if any(part in SKIP_DIRS for part in rel.parts):
            continue
        yield md_file, rel


def api_headers(token: str) -> dict:
    return {"Authorization": f"Bearer {token}", "Content-Type": "application/json"}


def load_existing_index(api_url: str, token: str) -> dict[str, str]:
    """Maps migration_source_path -> node id, so re-runs are idempotent."""
    index: dict[str, str] = {}
    resp = requests.get(f"{api_url}/nodes", params={"limit": 1000}, headers=api_headers(token), timeout=30)
    resp.raise_for_status()
    for item in resp.json()["items"]:
        detail = requests.get(f"{api_url}/nodes/{item['id']}", headers=api_headers(token), timeout=30)
        detail.raise_for_status()
        for prop in detail.json().get("properties", []):
            if prop["key"] == "migration_source_path" and prop.get("value_text"):
                index[prop["value_text"]] = item["id"]
    return index


def create_alias(api_url: str, token: str, node_id: str, alias: str) -> str:
    resp = requests.post(f"{api_url}/nodes/{node_id}/aliases", json={"alias": alias}, headers=api_headers(token), timeout=30)
    if resp.status_code == 409:
        return "already exists"
    resp.raise_for_status()
    return "created"


@click.command()
@click.option(
    "--vault-root",
    required=True,
    type=click.Path(exists=True, file_okay=False, path_type=Path),
    help="Root of the markdown vault to import (read-only — never modified).",
)
@click.option("--api-url", default="http://127.0.0.1:47823", show_default=True)
@click.option("--token", required=True, envvar="NOTEAPP_SYSTEM_TOKEN", help="A 'system'-scoped token.")
@click.option("--dry-run", is_flag=True, help="Print what would happen without writing anything.")
@click.option("--update-existing", is_flag=True, help="PATCH already-imported notes' content instead of leaving it alone.")
def main(vault_root: Path, api_url: str, token: str, dry_run: bool, update_existing: bool):
    click.echo(f"Scanning {vault_root} ...")
    existing = {} if dry_run else load_existing_index(api_url, token)

    created = updated = skipped = failed = 0
    node_ids: dict[Path, str] = {}  # rel path -> node id, for passes 2/3

    # ---- pass 1: create/update node content ----
    for md_file, rel in walk_vault(vault_root):
        source_path = str(rel).replace("\\", "/")
        try:
            post = frontmatter.load(md_file)
        except Exception as e:
            # Malformed/non-standard YAML frontmatter shouldn't take down an
            # otherwise-unrelated file's import — fall back to treating the
            # whole file as plain content with no frontmatter metadata.
            click.echo(f"  WARNING: unparseable frontmatter in {source_path} ({e}) — importing as plain content")
            failed += 1
            post = frontmatter.Post(md_file.read_text(encoding="utf-8"))
        vault_code = extract_vault_code(md_file.stem)
        title = derive_title(post.metadata, md_file.stem)
        node_type = infer_node_type(rel)
        existing_id = existing.get(source_path)

        if dry_run:
            action = "would update" if existing_id else "would create"
            click.echo(f"  {action} [{node_type}]{f' ({vault_code})' if vault_code else ''}: {source_path} -> \"{title}\"")
            continue

        properties = [{"key": "migration_source_path", "value_type": "text", "value_text": source_path}]

        if existing_id:
            node_ids[rel] = existing_id
            if not update_existing:
                skipped += 1
                click.echo(f"  skip content (already imported): {source_path}")
                continue
            payload = {"title": title, "content": post.content, "properties": properties}
            resp = requests.patch(f"{api_url}/nodes/{existing_id}", json=payload, headers=api_headers(token), timeout=30)
            resp.raise_for_status()
            updated += 1
            click.echo(f"  updated: {source_path}")
        else:
            payload = {"title": title, "node_type": node_type, "content": post.content, "properties": properties}
            if vault_code:
                payload["vault_code"] = vault_code
            resp = requests.post(f"{api_url}/nodes", json=payload, headers=api_headers(token), timeout=30)
            resp.raise_for_status()
            node_ids[rel] = resp.json()["id"]
            created += 1
            click.echo(f"  created: {source_path}")

    if dry_run:
        click.echo(f"\nDone (dry run). frontmatter_warnings={failed}")
        return

    # ---- pass 2: alias every node under its raw filename forms ----
    click.echo("\nCreating aliases (filename stem + vault-relative path) ...")
    for rel, node_id in node_ids.items():
        stem_alias = Path(rel).stem  # e.g. "[WK05]-kai-preferences"
        path_alias = str(Path(rel).with_suffix("")).replace("\\", "/")  # e.g. "Infrastructure/[IN02]-model-routing"
        for alias in {stem_alias, path_alias}:  # dedupes root-level files where these are identical
            result = create_alias(api_url, token, node_id, alias)
            click.echo(f"  alias '{alias}' -> {rel}: {result}")

    # ---- pass 3: force re-resolution now that every alias exists ----
    click.echo("\nRe-resolving links (forcing a no-op content re-save per node) ...")
    for md_file, rel in walk_vault(vault_root):
        node_id = node_ids.get(rel)
        if not node_id:
            continue
        try:
            post = frontmatter.load(md_file)
            content = post.content
        except Exception:
            content = md_file.read_text(encoding="utf-8")
        resp = requests.patch(f"{api_url}/nodes/{node_id}", json={"content": content}, headers=api_headers(token), timeout=30)
        resp.raise_for_status()

    click.echo(f"\nDone. created={created} updated={updated} skipped_content={skipped} frontmatter_warnings={failed}")


if __name__ == "__main__":
    main()
