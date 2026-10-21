// Thin wrappers over the Rust commands in src-tauri/src.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauri } from "./api";
import type { InstalledTool, Platform, Resolved, Tool, User } from "./types";

export const platform: Platform = navigator.userAgent.includes("Mac") ? "mac" : "win";

export interface RunInfo {
  slug: string;
  name: string;
  pid: number;
  /** Unix seconds. */
  startedAt: number;
  window: "shown" | "hidden";
}

export interface Progress {
  slug: string;
  stage: "preparing" | "downloading" | "installing" | "done" | "error";
  received: number;
  total: number | null;
  message?: string;
}

const on = <T,>(event: string, cb: (p: T) => void): Promise<UnlistenFn> =>
  isTauri ? listen<T>(event, (e) => cb(e.payload)) : Promise.resolve(() => {});

/** Where a tool installs from: its install source, else the GitHub repo in repoUrl. */
export function sourceOf(tool: Tool): string {
  if (tool.install?.type === "github") return tool.install.repo;
  if (tool.install?.type === "manifest") return tool.install.url;
  const m = tool.repoUrl.match(/github\.com\/([^/]+\/[^/#?]+)/);
  return m ? m[1].replace(/\.git$/, "") : "";
}

export const native = {
  library: (): Promise<InstalledTool[]> => (isTauri ? invoke("library_list") : Promise.resolve([])),
  resolve: (tool: Tool) => invoke<Resolved>("tool_resolve", { slug: tool.slug, source: sourceOf(tool) }),
  install: (tool: Tool) => invoke<InstalledTool>("tool_install", { slug: tool.slug, source: sourceOf(tool) }),
  /** Start it, or bring its window to the front if it's running. */
  open: (slug: string) => invoke<void>("tool_open", { slug }),
  hide: (slug: string) => invoke<void>("tool_hide", { slug }),
  close: (slug: string) => invoke<void>("tool_close", { slug }),
  running: (): Promise<RunInfo[]> => (isTauri ? invoke("tool_running") : Promise.resolve([])),
  uninstall: (slug: string) => invoke<void>("tool_uninstall", { slug }),
  shortcut: (slug: string) => invoke<string>("tool_shortcut", { slug }),
  reveal: (slug: string) => invoke<void>("tool_reveal", { slug }),

  onProgress: (cb: (p: Progress) => void) => on<Progress>("install-progress", cb),
  onRunning: (cb: (r: RunInfo[]) => void) => on<RunInfo[]>("running-changed", cb),
  onLibrary: (cb: () => void) => on<null>("library-changed", () => cb()),
  onUpdated: (cb: (p: { slug: string; name: string; version: string }) => void) => on("tool-updated", cb),
  onRoute: (cb: (p: { kind: string; slug: string }) => void) => on("open-route", cb),

  // Sign-in through the browser, handed back over toolbay://auth?code=...
  signIn: () => invoke<void>("auth_start"),
  signOut: () => invoke<void>("auth_sign_out"),
  me: (): Promise<User | null> => (isTauri ? invoke("auth_user") : Promise.resolve(null)),
  onAuth: (cb: (u: User | null) => void) => on<User | null>("auth-changed", cb),
  onError: (cb: (m: string) => void) => on<string>("auth-error", cb),

  otherComputersTools: (): Promise<{ slug: string; computer: string }[]> =>
    isTauri ? invoke("sync_other_tools") : Promise.resolve([]),
  onSync: (cb: () => void) => on<null>("sync-changed", () => cb()),

  settings: (): Promise<Record<string, string>> => (isTauri ? invoke("settings_get") : Promise.resolve({})),
  setSetting: (key: string, value: string) => invoke<void>("settings_set", { key, value }),
};

export function formatSize(bytes?: number | null) {
  if (!bytes) return "";
  if (bytes > 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  return `${Math.max(1, Math.round(bytes / 1024 ** 2))} MB`;
}

export function uptime(startedAt: number, now = Date.now()) {
  const s = Math.max(0, Math.floor(now / 1000 - startedAt));
  if (s < 60) return "just started";
  const m = Math.floor(s / 60);
  if (m < 60) return `for ${m} min`;
  const h = Math.floor(m / 60);
  return `for ${h} h ${m % 60} min`;
}
