// Thin wrappers over the Rust commands in src-tauri/src.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauri } from "./api";
import type { InstalledTool, Platform, Tool, User } from "./types";

export const platform: Platform = navigator.userAgent.includes("Mac") ? "mac" : "win";

export interface Progress {
  slug: string;
  stage: "downloading" | "extracting" | "installing" | "done" | "error";
  received: number;
  total: number | null;
  message?: string;
}

export interface FlowStatus {
  state: "off" | "idle" | "recording" | "transcribing" | "error" | "unavailable";
  modelReady: boolean;
  modelPath: string | null;
  hotkey: string;
  lastText: string | null;
  message: string | null;
}

export const native = {
  library: () => (isTauri ? invoke<InstalledTool[]>("library_list") : Promise.resolve([])),
  install: (tool: Tool) =>
    invoke<InstalledTool>("tool_install", {
      slug: tool.slug,
      name: tool.name,
      version: tool.version,
      url: tool.downloads[platform] ?? "",
      launch: tool.launch?.[platform] ?? null,
    }),
  launch: (slug: string) => invoke<void>("tool_launch", { slug }),
  uninstall: (slug: string) => invoke<void>("tool_uninstall", { slug }),
  reveal: (slug: string) => invoke<void>("tool_reveal", { slug }),
  installSkill: (slug: string) => invoke<string>("skill_install", { slug }),
  onProgress: (cb: (p: Progress) => void): Promise<UnlistenFn> =>
    isTauri ? listen<Progress>("install-progress", (e) => cb(e.payload)) : Promise.resolve(() => {}),

  // Auth: browser sign-in, handed back over toolbay://auth?code=...
  signIn: () => invoke<void>("auth_start"),
  signOut: () => invoke<void>("auth_sign_out"),
  me: () => (isTauri ? invoke<User | null>("auth_user") : Promise.resolve(null)),
  onAuth: (cb: (u: User | null) => void): Promise<UnlistenFn> =>
    isTauri ? listen<User | null>("auth-changed", (e) => cb(e.payload)) : Promise.resolve(() => {}),
  onAuthError: (cb: (m: string) => void): Promise<UnlistenFn> =>
    isTauri ? listen<string>("auth-error", (e) => cb(e.payload)) : Promise.resolve(() => {}),

  settings: (): Promise<Record<string, string>> => (isTauri ? invoke("settings_get") : Promise.resolve({})),
  setSetting: (key: string, value: string) => invoke<void>("settings_set", { key, value }),

  // Toolbay Flow (built-in dictation)
  flowStatus: () => invoke<FlowStatus>("flow_status"),
  flowEnable: (enabled: boolean) => invoke<FlowStatus>("flow_enable", { enabled }),
  flowDownloadModel: () => invoke<FlowStatus>("flow_download_model"),
  onFlow: (cb: (s: FlowStatus) => void): Promise<UnlistenFn> =>
    isTauri ? listen<FlowStatus>("flow-status", (e) => cb(e.payload)) : Promise.resolve(() => {}),
};
