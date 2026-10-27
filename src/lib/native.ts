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

/** Browser preview only (`npm run dev`, open with ?demo): pretend Toolbay Flow is installed and
 * open, so screens can be designed without the desktop app. Never active inside Tauri. */
const demo = !isTauri && new URLSearchParams(location.search).has("demo");
const demoWindow = new URLSearchParams(location.search).get("demo") === "hidden" ? "hidden" : "shown";
const demoLibrary = [
  { slug: "toolbay-flow", name: "Toolbay Flow", version: "0.4.14", repo: "sriptcollector/toolbay-flow" },
  { slug: "cereal", name: "Cereal", version: "0.4.2", repo: "sriptcollector/cereal" },
] as unknown as InstalledTool[];
const demoRunning: RunInfo[] = [
  { slug: "toolbay-flow", name: "Toolbay Flow", pid: 1, startedAt: Date.now() / 1000 - 600, window: demoWindow },
];

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
  library: (): Promise<InstalledTool[]> =>
    isTauri ? invoke("library_list") : Promise.resolve(demo ? demoLibrary : []),
  resolve: (tool: Tool) => invoke<Resolved>("tool_resolve", { slug: tool.slug, source: sourceOf(tool) }),
  install: (tool: Tool) => invoke<InstalledTool>("tool_install", { slug: tool.slug, source: sourceOf(tool) }),
  /** Start it, or bring its window to the front if it's running. */
  open: (slug: string) => invoke<void>("tool_open", { slug }),
  hide: (slug: string) => invoke<void>("tool_hide", { slug }),
  close: (slug: string) => invoke<void>("tool_close", { slug }),
  running: (): Promise<RunInfo[]> => (isTauri ? invoke("tool_running") : Promise.resolve(demo ? demoRunning : [])),
  uninstall: (slug: string) => invoke<void>("tool_uninstall", { slug }),
  /** Keep an installed tool when its slug changes on toolbay.ai. */
  rename: (from: string, to: string) => invoke<void>("library_rename", { from, to }),
  shortcut: (slug: string) => invoke<string>("tool_shortcut", { slug }),
  reveal: (slug: string) => invoke<void>("tool_reveal", { slug }),

  onProgress: (cb: (p: Progress) => void) => on<Progress>("install-progress", cb),
  onRunning: (cb: (r: RunInfo[]) => void) => on<RunInfo[]>("running-changed", cb),
  onLibrary: (cb: () => void) => on<null>("library-changed", () => cb()),
  onUpdated: (cb: (p: { slug: string; name: string; version: string }) => void) => on("tool-updated", cb),
  /** A newer version was found for a tool that's open; it waits for "Update now" or for the tool to close. */
  onUpdateReady: (cb: (p: { slug: string; name: string; version: string }) => void) => on("tool-update-ready", cb),
  pendingUpdates: (): Promise<Record<string, string>> => (isTauri ? invoke("tool_pending_updates") : Promise.resolve({})),
  /** Close the tool if it's open, install the new version, open it again. */
  updateNow: (slug: string) => invoke<InstalledTool>("tool_update_now", { slug }),
  /** Screens queued in Rust (first-run "Install <tool>?", toolbay://install/<slug>). */
  routeTake: (): Promise<{ kind: string; slug: string; firstRun?: boolean }[]> =>
    isTauri ? invoke("route_take") : Promise.resolve([]),
  onRoutePending: (cb: () => void) => on<null>("route-pending", () => cb()),

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
