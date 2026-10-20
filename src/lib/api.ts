// Catalog client. Requests go through Rust (no CORS, token never touches JS),
// and fall back to bundled fixtures while the site API is not live.
import { invoke } from "@tauri-apps/api/core";
import toolsFixture from "../fixtures/tools.json";
import skillsFixture from "../fixtures/skills.json";
import postsFixture from "../fixtures/posts.json";
import type { Post, Review, Skill, Tool } from "./types";

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

export const api = {
  tools: () => withFallback<Tool[]>("/api/v1/tools", toolsFixture.tools as Tool[], (j) => j.tools),
  tool: (slug: string) =>
    withFallback<Tool | undefined>(
      `/api/v1/tools/${slug}`,
      (toolsFixture.tools as Tool[]).find((t) => t.slug === slug),
      (j) => j.tool ?? j,
    ),
  skills: () => withFallback<Skill[]>("/api/v1/skills", skillsFixture.skills as Skill[], (j) => j.skills),
  reviews: (slug: string) =>
    withFallback<Review[]>(`/api/v1/tools/${slug}/reviews`, [], (j) => j.reviews),
  postReview: (slug: string, rating: number, body: string) =>
    request<{ review: Review }>("POST", `/api/v1/tools/${slug}/reviews`, { rating, body }),
  posts: () => withFallback<Post[]>("/api/v1/community/posts", postsFixture.posts as Post[], (j) => j.posts),
  createPost: (body: string, toolSlug?: string) =>
    request<{ post: Post }>("POST", "/api/v1/community/posts", { body, toolSlug: toolSlug ?? null }),
};
