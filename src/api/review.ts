import { apiFetch } from "./client";
import type { ChangelogEntryDto, ReviewItemDto } from "../types/memory";
import type { WriteResultDto } from "../types/node";

export function listReviewItems(status?: string): Promise<{ items: ReviewItemDto[] }> {
  const qs = status ? `?status=${encodeURIComponent(status)}` : "";
  return apiFetch(`/review${qs}`);
}

export function approveReviewItem(id: string): Promise<WriteResultDto> {
  return apiFetch(`/review/${id}/approve`, { method: "POST" });
}

export function rejectReviewItem(id: string, reason?: string): Promise<WriteResultDto> {
  return apiFetch(`/review/${id}/reject`, { method: "POST", body: JSON.stringify({ reason }) });
}

export function applyReviewItem(id: string): Promise<WriteResultDto> {
  return apiFetch(`/review/${id}/apply`, { method: "POST" });
}

export function listChangelog(actor?: string, limit = 100): Promise<{ items: ChangelogEntryDto[] }> {
  const params = new URLSearchParams();
  if (actor) params.set("actor", actor);
  params.set("limit", String(limit));
  return apiFetch(`/changelog?${params.toString()}`);
}
