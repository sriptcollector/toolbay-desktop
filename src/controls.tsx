// One set of controls for a tool, used everywhere (featured hero, tool page, Library, Running now),
// so a tool looks and behaves the same in every state:
//   not installed -> Install        installing -> progress        installed -> Open
//   running -> Running · Close · [Switch to]
import { useEffect, useState } from "react";
import type { Hub } from "./App";
import { isTauri } from "./lib/api";
import { formatSize, native, platform } from "./lib/native";
import type { Resolved, Tool } from "./lib/types";

const resolvedCache = new Map<string, Resolved | null>();

export function useResolved(tool: Tool | undefined) {
  const [r, setR] = useState<Resolved | null | undefined>(tool ? resolvedCache.get(tool.slug) : undefined);
  useEffect(() => {
    if (!tool || resolvedCache.has(tool.slug)) return;
    if (!isTauri) return setR(null);
    native
      .resolve(tool)
      .then((x) => {
        resolvedCache.set(tool.slug, x);
        setR(x);
      })
      .catch(() => {
        resolvedCache.set(tool.slug, null);
        setR(null);
      });
  }, [tool?.slug]); // eslint-disable-line react-hooks/exhaustive-deps
  return r;
}

export function totalSize(r: Resolved | null | undefined) {
  if (!r?.entry.size) return null;
  return r.entry.size + r.manifest.files.reduce((a, f) => a + (f.size ?? 0), 0);
}

/** "● Running", one line, never wraps. */
export function RunStatus() {
  return (
    <span className="status live">
      <span className="dot" />
      Running
    </span>
  );
}

export function ToolControls({ hub, tool, size = "row" }: { hub: Hub; tool: Tool; size?: "hero" | "row" }) {
  const r = useResolved(tool);
  const installed = hub.library.some((l) => l.slug === tool.slug);
  const run = hub.isRunning(tool.slug);
  const p = hub.progress[tool.slug];
  const busy = hub.busy[tool.slug];
  const big = size === "hero";
  const update = installed ? hub.pendingUpdates[tool.slug] : undefined;
  const updateBtn = update && (
    <button className="btn small" disabled={!!busy} onClick={() => hub.update(tool)} title={`Closes ${tool.name}, updates it, and opens it again`}>
      Update to {update}
    </button>
  );
  const primary = `btn primary ${big ? "big" : ""}`;
  const secondary = "btn small";

  if (p) return <InstallProgress hub={hub} slug={tool.slug} big={big} />;

  if (run) {
    return (
      <div className={`controls controls-${size}`}>
        <div className="controls-main">
          <button className={primary} disabled={!!busy} onClick={() => hub.open(tool)}>
            {busy === "opening" ? "Opening…" : "Switch to"}
          </button>
        </div>
        <div className="controls-sub">
          <RunStatus />
          {updateBtn}
          <button className={secondary} disabled={!!busy} onClick={() => hub.close(tool)}>
            {busy === "closing" ? "Closing…" : "Close"}
          </button>
        </div>
      </div>
    );
  }

  let button: React.ReactNode;
  let note: React.ReactNode = null;
  if (installed) {
    button = (
      <button className={primary} disabled={!!busy} onClick={() => hub.open(tool)}>
        {busy === "opening" ? "Opening…" : "Open"}
      </button>
    );
  } else if (!tool.platforms.includes(platform)) {
    button = (
      <button className={primary} disabled>
        Not on {platform === "mac" ? "Mac" : "Windows"} yet
      </button>
    );
  } else if (r === null) {
    button = (
      <button className={primary} disabled>
        Coming soon
      </button>
    );
    note = "Not ready to download yet.";
  } else {
    const total = totalSize(r);
    button = (
      <button className={primary} disabled={r === undefined} onClick={() => hub.install(tool)}>
        Install
      </button>
    );
    note = `Free${total ? ` · ${formatSize(total)}` : ""}`;
  }
  return (
    <div className={`controls controls-${size}`}>
      <div className="controls-main">{button}</div>
      {updateBtn && <div className="controls-sub">{updateBtn}</div>}
      {big && note && <div className="controls-sub note">{note}</div>}
    </div>
  );
}

function InstallProgress({ hub, slug, big }: { hub: Hub; slug: string; big: boolean }) {
  const p = hub.progress[slug];
  const pct = p.total ? Math.min(1, p.received / p.total) : null;
  const downloading = p.stage === "downloading" && pct != null;
  const label =
    p.stage === "installing"
      ? "Installing…"
      : p.stage === "preparing"
        ? "Getting ready…"
        : pct != null
          ? `Downloading ${Math.round(pct * 100)}%`
          : `Downloading ${formatSize(p.received)}`;
  return (
    <div className={`controls controls-${big ? "hero" : "row"}`}>
      <div className="install-progress" role="progressbar" aria-valuenow={pct != null ? Math.round(pct * 100) : undefined}>
        <div className="install-progress-label">
          <span>{label}</span>
          {big && p.total && p.stage === "downloading" && (
            <span className="muted">
              {formatSize(p.received)} / {formatSize(p.total)}
            </span>
          )}
        </div>
        <div className={`progress ${downloading ? "" : "indeterminate"}`}>
          <div style={downloading ? { transform: `scaleX(${pct})` } : undefined} />
        </div>
      </div>
    </div>
  );
}
