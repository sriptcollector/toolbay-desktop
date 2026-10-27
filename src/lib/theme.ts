// Appearance: "system" (default, follows the OS live), "light" or "dark".
// Sets <html data-theme="light|dark">; the palettes live in styles.css.
// Saved in the native settings store (key "appearance") and cached in localStorage so
// main.tsx can apply it before the first render (no dark/light flash).
import { useSyncExternalStore } from "react";
import { native } from "./native";

export type Appearance = "system" | "light" | "dark";
export type Theme = "light" | "dark";

const CACHE_KEY = "appearance";
const mq = typeof matchMedia === "function" ? matchMedia("(prefers-color-scheme: dark)") : null;
const listeners = new Set<() => void>();

const valid = (v: unknown): Appearance | null => (v === "system" || v === "light" || v === "dark" ? v : null);

function readCache(): Appearance {
  try {
    return valid(localStorage.getItem(CACHE_KEY)) ?? "system";
  } catch {
    return "system";
  }
}

let appearance: Appearance = readCache();

const resolve = (): Theme => (appearance === "system" ? (mq && !mq.matches ? "light" : "dark") : appearance);

function apply() {
  const theme = resolve();
  document.documentElement.dataset.theme = theme;
  // The Dock / taskbar icon follows too.
  if ("__TAURI_INTERNALS__" in window) {
    import("@tauri-apps/api/core").then(({ invoke }) => invoke("set_app_icon", { dark: theme === "dark" })).catch(() => {});
  }
  listeners.forEach((l) => l());
}

/** Call once before rendering: applies the cached choice and follows OS changes while on System. */
export function initTheme() {
  apply();
  mq?.addEventListener("change", () => {
    if (appearance === "system") apply();
  });
}

/** Switch now; also saves it unless it came from the store. */
export function setAppearance(a: Appearance, save = true) {
  appearance = a;
  try {
    localStorage.setItem(CACHE_KEY, a);
  } catch {
    /* cache is optional */
  }
  apply();
  if (save) native.setSetting("appearance", a).catch(() => {});
}

/** At app start: the saved setting wins over the localStorage cache. */
export function loadAppearance() {
  native
    .settings()
    .then((s) => {
      const a = valid(s.appearance);
      if (a && a !== appearance) setAppearance(a, false);
    })
    .catch(() => {});
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export function useAppearance(): Appearance {
  return useSyncExternalStore(subscribe, () => appearance);
}

export function useResolvedTheme(): Theme {
  return useSyncExternalStore(subscribe, resolve);
}
