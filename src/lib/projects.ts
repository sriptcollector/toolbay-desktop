// src/lib/projects.ts for toolbay-desktop: the Upload project API (toolbay.ai /api/v1/projects).
// Uses the same Rust-side api_request as lib/api.ts (Bearer token never touches JS).
import { invoke } from "@tauri-apps/api/core";

export interface Project {
  id: string;
  repoUrl: string;
  repo: string;
  commitSha: string;
  name: string;
  slug: string | null;
  kind: string | null;
  kindLabel: string | null;
  status: "queued" | "checking" | "building" | "ready" | "failed" | "unsupported";
  step: string;
  message: string | null;
  version: string | null;
  license: string | null;
  privacy: {
    status: "passed" | "pending" | "failed";
    summary: string;
    details: string[];
    checks: { id: string; label: string; status: "passed" | "warning" | "failed"; summary: string; details: string[] }[];
  };
  listing: "private" | "pending_review" | "public" | "rejected";
  reviewNote: string | null;
  manifestUrl: string | null;
  downloads: { win: string | null; mac: string | null };
  iconUrl: string | null;
  logsUrl: string | null;
  toolSlug: string | null;
  /** Set when this project is a fork of a tool on Toolbay. */
  forkedFrom?: { toolSlug: string; builderHandle: string } | null;
  /** "github" or "package" (a zip uploaded from the hub or the site). */
  source?: string;
  aiScan?: { status: string; summary: string } | null;
  holdReasons?: string[];
  autoPublished?: boolean;
  createdAt: string;
  updatedAt: string;
}

export async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await invoke<{ status: number; body: string }>("api_request", {
    method,
    path,
    body: body === undefined ? null : JSON.stringify(body),
  });
  let json: any = null;
  try {
    json = JSON.parse(res.body);
  } catch {
    throw new Error("Couldn't reach toolbay.ai. Check your internet connection and try again.");
  }
  if (res.status === 401) throw new Error("Sign in again to upload projects.");
  if (res.status < 200 || res.status >= 300) throw new Error(json?.error || "That didn't work. Try again.");
  return json as T;
}

export const projectsApi = {
  all: () => call<{ projects: Project[] }>("GET", "/api/v1/projects").then((j) => j.projects),
  get: (id: string) => call<{ project: Project }>("GET", `/api/v1/projects/${id}`).then((j) => j.project),
  create: (repoUrl: string, opts: { listPublicly?: boolean; rebuild?: boolean } = {}) =>
    call<{ project: Project }>("POST", "/api/v1/projects", { repoUrl, ...opts }).then((j) => j.project),
  list: (id: string) => call<{ project: Project }>("POST", `/api/v1/projects/${id}/listing`),
  remove: (id: string) => call<{ ok: true }>("DELETE", `/api/v1/projects/${id}`),
};
