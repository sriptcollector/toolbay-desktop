// Catalog client. Requests go through Rust (no CORS, token never touches JS),
// and fall back to bundled fixtures while the site API is not live.
import { invoke } from "@tauri-apps/api/core";
import toolsFixture from "../fixtures/tools.json";
import postsFixture from "../fixtures/posts.json";
import type { ForkSummary, Post, Profile, Review, Tool } from "./types";
import type { Project } from "./projects";

export const isTauri = "__TAURI_INTERNALS__" in window;

interface RawResponse {
  status: number;
  body: string;
}

export interface Result<T> {
  data: T;
  live: boolean;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  if (!isTauri) throw new Error("not running inside the desktop app");
  const res = await invoke<RawResponse>("api_request", {
    method,
    path,
    body: body === undefined ? null : JSON.stringify(body),
  });
  if (res.status < 200 || res.status >= 300) throw new Error(`HTTP ${res.status}`);
  return JSON.parse(res.body) as T; // throws on HTML, which counts as "not live"
}

async function withFallback<T>(path: string, fallback: T, pick: (j: any) => T): Promise<Result<T>> {
  try {
    const json = await request<any>("GET", path);
    const data = pick(json);
    if (data === undefined || data === null) throw new Error("unexpected shape");
    return { data, live: true };
  } catch {
    return { data: fallback, live: false };
  }
}

// Logos bundled with the app, used when the catalog has none yet.
const BUNDLED_ICONS: Record<string, string> = { "toolbay-flow": "/tools/toolbay-flow.png" };

/** Fill gaps in catalog entries so every screen can rely on the same shape. */
export function normalize(t: Tool & { replaces?: string | null }): Tool {
  const inspiredBy =
    t.inspiredBy ?? (t.replaces ? { name: t.replaces, company: `the makers of ${t.replaces}` } : null);
  let iconUrl = t.iconUrl || BUNDLED_ICONS[t.slug] || "";
  if (iconUrl.startsWith("/") && !BUNDLED_ICONS[t.slug]) iconUrl = `https://toolbay.ai${iconUrl}`;
  return { ...t, inspiredBy, iconUrl, screenshots: t.screenshots ?? [], platforms: t.platforms ?? [] };
}

/** The catalog shipped inside the app, for an instant first screen. */
export const bundledTools = () => (toolsFixture.tools as Tool[]).map(normalize);

export const api = {
  tools: () =>
    withFallback<Tool[]>("/api/v1/tools", (toolsFixture.tools as Tool[]).map(normalize), (j) =>
      Array.isArray(j.tools) ? j.tools.map(normalize) : undefined,
    ),
  tool: (slug: string) =>
    withFallback<Tool | undefined>(
      `/api/v1/tools/${slug}`,
      (toolsFixture.tools as Tool[]).find((t) => t.slug === slug),
      (j) => j.tool ?? j,
    ),
  reviews: (slug: string) =>
    withFallback<Review[]>(`/api/v1/tools/${slug}/reviews`, [], (j) => j.reviews),
  postReview: (slug: string, rating: number, body: string) =>
    request<{ review: Review }>("POST", `/api/v1/tools/${slug}/reviews`, { rating, body }),
  posts: () => withFallback<Post[]>("/api/v1/community/posts", postsFixture.posts as Post[], (j) => j.posts),
  createPost: (body: string) => request<{ post: Post }>("POST", "/api/v1/community/posts", { body }),
  /** A person's profile, or null when there's no such user (or toolbay.ai can't be reached). */
  user: async (handle: string): Promise<Profile | null> => {
    try {
      const { user } = await request<{ user: Profile }>("GET", `/api/v1/users/${encodeURIComponent(handle)}`);
      return {
        ...user,
        pinned: (user.pinned ?? []).map((p) => (p.type === "tool" ? { ...p, tool: normalize(p.tool) } : p)),
        apps: (user.apps ?? []).map(normalize),
        forks: (user.forks ?? []).map(normalize),
        posts: user.posts ?? [],
        repos: user.repos ?? [],
        links: user.links ?? { website: null, github: null, x: null },
      };
    } catch {
      return null;
    }
  },
  forks: (slug: string) =>
    withFallback<{ count: number; forks: ForkSummary[] }>(`/api/v1/tools/${slug}/forks`, { count: 0, forks: [] }, (j) =>
      typeof j.count === "number" ? j : undefined,
    ),
  fork: (slug: string) => callApi<ForkStart>("POST", `/api/v1/tools/${slug}/fork`),
  claimFork: (slug: string) => callApi<{ project: Project | null }>("POST", `/api/v1/tools/${slug}/fork/claim`),
};

export type ForkStart =
  | { mode: "done"; project: Project }
  | { mode: "github"; url: string }
  | { mode: "app"; url: string };

/** Like request(), but turns the API's plain-words {error} into the thrown message. */
export async function callApi<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await invoke<RawResponse>("api_request", {
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
  if (res.status === 401) throw new Error("Sign in first, then try again.");
  if (res.status < 200 || res.status >= 300) throw new Error(json?.error || "That didn't work. Try again.");
  return json as T;
}
