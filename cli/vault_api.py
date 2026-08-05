#!/usr/bin/env python3
"""CLI bridge to the noteapp local API.

This is the concrete way an external AI agent loads compiled memory context
at session start and writes back through the review queue or direct routes —
if the memory compiler only existed as a UI feature, nothing would call it.

Auth: reads the agent token from NOTEAPP_AGENT_TOKEN (never a --token flag,
so it never lands in shell history or a process listing). Base URL defaults
to the app's fixed local port; override with NOTEAPP_API_URL if needed.

Usage:
    pip install click requests
    export NOTEAPP_AGENT_TOKEN=<the 'agent' token printed on first app launch>
    python vault_api.py context
    python vault_api.py note get <id>
    python vault_api.py note search "query"
    python vault_api.py review list --status pending
"""

import json
import os
import sys

import click
import requests

BASE_URL = os.environ.get("NOTEAPP_API_URL", "http://127.0.0.1:47823")


def _headers() -> dict:
    token = os.environ.get("NOTEAPP_AGENT_TOKEN")
    if not token:
        raise click.ClickException(
            "NOTEAPP_AGENT_TOKEN is not set - export the 'agent' token printed on first app launch."
        )
    return {"Authorization": f"Bearer {token}"}


def _request(method: str, path: str, **kwargs):
    try:
        resp = requests.request(method, f"{BASE_URL}{path}", headers=_headers(), timeout=30, **kwargs)
    except requests.ConnectionError as e:
        raise click.ClickException(f"could not reach {BASE_URL} — is the app running? ({e})")
    if not resp.ok:
        try:
            detail = resp.json().get("error", resp.text)
        except ValueError:
            detail = resp.text
        raise click.ClickException(f"{resp.status_code}: {detail}")
    if resp.status_code == 204 or not resp.content:
        return None
    return resp.json()


def _print(data) -> None:
    click.echo(json.dumps(data, indent=2, ensure_ascii=False))


@click.group()
def cli():
    """noteapp vault CLI bridge."""


# ---- memory ----


@cli.command("context")
@click.option("--budget-hot", type=int, default=2200, help="Character budget for hot_memory.")
@click.option("--budget-profile", type=int, default=1375, help="Character budget for user_profile.")
def memory_context(budget_hot: int, budget_profile: int):
    """Fetch the compiled memory context — the actual thing to load at session start."""
    data = _request(
        "GET",
        "/memory/context",
        params={"budget_hot": budget_hot, "budget_profile": budget_profile},
    )
    _print(data)


# ---- notes ----


@cli.group()
def note():
    """Note (node) operations."""


@note.command("get")
@click.argument("note_id")
def note_get(note_id: str):
    _print(_request("GET", f"/nodes/{note_id}"))


@note.command("search")
@click.argument("query")
@click.option("--node-type", default=None)
@click.option("--limit", type=int, default=20)
def note_search(query: str, node_type: str | None, limit: int):
    params = {"q": query, "limit": limit}
    if node_type:
        params["node_type"] = node_type
    _print(_request("GET", "/search", params=params))


@note.command("create")
@click.option("--title", required=True)
@click.option("--content", default="")
@click.option("--content-file", type=click.File("r"), default=None, help="Read content from a file instead.")
@click.option("--node-type", default="page")
@click.option("--vault-code", default=None)
def note_create(title: str, content: str, content_file, node_type: str, vault_code: str | None):
    body = {
        "title": title,
        "node_type": node_type,
        "content": content_file.read() if content_file else content,
    }
    if vault_code:
        body["vault_code"] = vault_code
    _print(_request("POST", "/nodes", json=body))


@note.command("update")
@click.argument("note_id")
@click.option("--title", default=None)
@click.option("--content", default=None)
@click.option("--content-file", type=click.File("r"), default=None)
def note_update(note_id: str, title: str | None, content: str | None, content_file):
    body = {}
    if title is not None:
        body["title"] = title
    if content_file:
        body["content"] = content_file.read()
    elif content is not None:
        body["content"] = content
    if not body:
        raise click.ClickException("nothing to update — pass --title and/or --content")
    _print(_request("PATCH", f"/nodes/{note_id}", json=body))


@note.command("append")
@click.argument("note_id")
@click.argument("content_to_append")
def note_append(note_id: str, content_to_append: str):
    _print(_request("POST", f"/nodes/{note_id}/append", json={"content_to_append": content_to_append}))


# ---- review queue ----


@cli.group()
def review():
    """AI review queue — propose here; approve/apply stay human-gated in the app by default."""


@review.command("list")
@click.option("--status", default=None, type=click.Choice(["pending", "approved", "rejected", "applied"]))
def review_list(status: str | None):
    params = {"status": status} if status else {}
    _print(_request("GET", "/review", params=params))


@review.command("propose")
@click.option("--action", "proposed_action", required=True, type=click.Choice(["create", "update", "delete"]))
@click.option("--entity-type", required=True, type=click.Choice(["node", "hot_memory", "user_profile"]))
@click.option("--entity-id", default=None, help="Required for update/delete proposals.")
@click.option("--diff-json", required=True, help="JSON string describing the proposed change.")
@click.option("--reason", default=None)
@click.option("--confidence", default=None, type=click.Choice(["high", "medium", "low"]))
def review_propose(
    proposed_action: str,
    entity_type: str,
    entity_id: str | None,
    diff_json: str,
    reason: str | None,
    confidence: str | None,
):
    try:
        parsed_diff = json.loads(diff_json)
    except json.JSONDecodeError as e:
        raise click.ClickException(f"--diff-json is not valid JSON: {e}")
    body = {
        "proposed_action": proposed_action,
        "entity_type": entity_type,
        "entity_id": entity_id,
        "proposed_diff_json": parsed_diff,
        "reason": reason,
        "confidence": confidence,
    }
    _print(_request("POST", "/review", json=body))


if __name__ == "__main__":
    sys.exit(cli())
