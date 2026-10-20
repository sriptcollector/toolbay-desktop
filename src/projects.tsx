// Upload project: turn an open-source GitHub repo into an app in Toolbay (reached from Library).
// toolbay.ai builds it (POST /api/v1/projects); this screen shows live status, and Install uses the
// hub's own installer with the build's toolbay.json as the source, so a ready project installs,
// opens, closes and shows as running like any other tool.
import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Hub } from "./App";
import { isTauri } from "./lib/api";
import { projectsApi, type Project } from "./lib/projects";
import type { Tool } from "./lib/types";
import { ForkedFrom } from "./components";

const ACTIVE = ["queued", "checking", "building"];

const STATUS: Record<Project["status"], { text: string; cls: string }> = {
  queued: { text: "Waiting to start", cls: "" },
  checking: { text: "Checking", cls: "" },
  building: { text: "Building", cls: "" },
  ready: { text: "Ready", cls: "ok" },
  failed: { text: "Didn't build", cls: "bad" },
  unsupported: { text: "Not supported yet", cls: "bad" },
};

const LISTING: Record<Project["listing"], string> = {
  private: "Only you",
  pending_review: "Waiting for review",
  public: "Listed on Toolbay",
  rejected: "Not listed",
};

export const readyTools = (ps: Project[]) => ps.filter((p) => p.status === "ready" && p.manifestUrl).map(projectTool);

/** A ready project as a hub Tool, so install / open / close / Running now work unchanged. */
export function projectTool(p: Project): Tool {
  return {
    slug: `p-${p.id}`,
    name: p.name,
    tagline: p.kindLabel ?? "",
    description: "",
    iconUrl: p.iconUrl ?? "",
    screenshots: [],
    category: "My projects",
    repoUrl: p.repoUrl,
    license: p.license ?? "",
    version: p.version ?? "1.0.0",
    platforms: [p.downloads.win ? "win" : null, p.downloads.mac ? "mac" : null].filter(Boolean) as Tool["platforms"],
    priceCents: 0,
    openSource: true,
    privacy: { status: p.privacy.status, checkedAt: null, summary: p.privacy.summary },
    rating: null,
    reviewCount: 0,
    author: { name: "You", handle: "" },
    install: p.manifestUrl ? { type: "manifest", url: p.manifestUrl } : undefined,
  } as Tool;
}

function Steps({ p }: { p: Project }) {
  const steps = ["checking", "building", "ready"];
  const at = p.status === "queued" ? -1 : steps.indexOf(p.status);
  return (
    <div className="build-steps" aria-hidden>
      {steps.map((s, i) => (
        <span key={s} className={i < at ? "done" : i === at ? "now" : ""} />
      ))}
    </div>
  );
}

function Row({ hub, p, refresh }: { hub: Hub; p: Project; refresh: () => void }) {
  const [open, setOpen] = useState(false);
  const [noIcon, setNoIcon] = useState(false);
  const tool = projectTool(p);
  const installed = hub.library.some((l) => l.slug === tool.slug);
  const running = !!hub.isRunning(tool.slug);
  const busy = !!hub.progress[tool.slug];
  const bad = p.status === "failed" || p.status === "unsupported";
  const s = STATUS[p.status];

  const act = async (fn: () => Promise<unknown>, ok?: string) => {
    try {
      await fn();
      if (ok) hub.toast(ok);
      refresh();
    } catch (e) {
      hub.toast(String(e instanceof Error ? e.message : e));
    }
  };

  return (
    <div className="list-row project-row">
      {p.iconUrl && !noIcon ? (
        <img className="icon" src={p.iconUrl} alt="" onError={() => setNoIcon(true)} />
      ) : (
        <div className="icon">{p.name.charAt(0).toUpperCase()}</div>
      )}
      <div className="grow">
        <div className="title">
          {p.name} <span className={`chip ${s.cls}`}>{s.text}</span>
          {p.status === "ready" && <span className="chip">{LISTING[p.listing]}</span>}
        </div>
        {p.forkedFrom && <ForkedFrom hub={hub} from={p.forkedFrom} />}
        <div className="small muted mono">
          {p.repo} · {p.commitSha.slice(0, 7)}
          {p.kindLabel ? ` · ${p.kindLabel}` : ""}
        </div>
        {ACTIVE.includes(p.status) && (
          <>
            <div className="small">{p.step}…</div>
            <Steps p={p} />
          </>
        )}
        {p.message && <div className={`small ${bad ? "danger" : ""}`}>{p.message}</div>}
        {p.listing === "rejected" && p.reviewNote && <div className="small muted">Review note: {p.reviewNote}</div>}
        {p.privacy.checks.length > 0 && (
          <button className="link small privacy-toggle" onClick={() => setOpen(!open)}>
            Privacy check: {p.privacy.summary} {open ? "Hide" : "Details"}
          </button>
        )}
        {open && (
          <ul className="small privacy-list">
            {p.privacy.checks.map((c) => (
              <li key={c.id}>
                <span className={c.status === "passed" ? "ok" : c.status === "warning" ? "warn" : "danger"}>
                  {c.status === "passed" ? "✓" : c.status === "warning" ? "!" : "✕"}
                </span>{" "}
                <b>{c.label}:</b> {c.summary}
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="row-actions">
        {p.status === "ready" && !installed && (
          <button className="btn primary" disabled={busy || !tool.install} onClick={() => hub.install(tool)}>
            {busy ? "Installing…" : "Install"}
          </button>
        )}
        {p.status === "ready" && installed && (
          <button className="btn primary" onClick={() => hub.open(tool)}>
            {running ? "Show" : "Open"}
          </button>
        )}
        {p.status === "ready" && p.listing === "private" && (
          <button className="btn" onClick={() => act(() => projectsApi.list(p.id), "Sent for review. You'll see it here when it's listed.")}>
            List on Toolbay
          </button>
        )}
        {p.status === "failed" && (
          <button className="btn" onClick={() => act(() => projectsApi.create(p.repoUrl, { rebuild: true }))}>
            Try again
          </button>
        )}
        {p.logsUrl && (bad || ACTIVE.includes(p.status)) && (
          <button className="btn" onClick={() => isTauri && openUrl(p.logsUrl!)}>
            Build log
          </button>
        )}
      </div>
    </div>
  );
}

export function ProjectsPage({ hub }: { hub: Hub }) {
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [url, setUrl] = useState("");
  const [listPublicly, setListPublicly] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!hub.user || !isTauri) return;
    try {
      const all = await projectsApi.all();
      setProjects(all);
      hub.setProjectTools(readyTools(all));
    } catch {
      setProjects((cur) => cur ?? []);
    }
  }, [hub.user, hub.setProjectTools]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Live status while anything is building.
  useEffect(() => {
    if (!projects?.some((p) => ACTIVE.includes(p.status))) return;
    const t = window.setInterval(refresh, 5000);
    return () => window.clearInterval(t);
  }, [projects, refresh]);

  const submit = async () => {
    setError(null);
    if (!url.trim()) return setError("Paste a GitHub link first.");
    setBusy(true);
    try {
      await projectsApi.create(url.trim(), { listPublicly });
      setUrl("");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "That didn't start. Try again.");
    } finally {
      setBusy(false);
    }
  };

  if (!hub.user) {
    return (
      <>
        <h1>Upload a project</h1>
        <p className="lede">Turn an open-source project on GitHub into an app you can open from Toolbay.</p>
        <div className="empty">
          <button className="btn primary" onClick={hub.signIn}>
            Sign in to upload
          </button>
        </div>
      </>
    );
  }

  return (
    <>
      <button className="link small" onClick={() => hub.go({ page: "library" })}>
        ← Library
      </button>
      <h1>Upload a project</h1>
      <p className="lede">
        Paste a link to an open-source project on GitHub. Toolbay checks it for privacy and safety, builds it, and
        turns it into an app here.
      </p>
      <div className="panel upload-box">
        <div className="upload-row">
          <input
            className="input grow mono"
            placeholder="https://github.com/owner/project"
            value={url}
            spellCheck={false}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && !busy && submit()}
          />
          <button className="btn primary" onClick={submit} disabled={busy}>
            {busy ? "Starting…" : "Make it an app"}
          </button>
        </div>
        <label className="small check">
          <input type="checkbox" checked={listPublicly} onChange={(e) => setListPublicly(e.target.checked)} />
          Also list it on Toolbay for everyone (after a quick review)
        </label>
        {error && <div className="small danger">{error}</div>}
        <div className="small muted">
          Works with web apps, Tauri and Electron apps, and Python or Node tools. It needs an open-source license. You
          can always install your own build.
        </div>
      </div>
      <h2>My projects</h2>
      {projects === null ? (
        <div className="empty">Loading…</div>
      ) : projects.length === 0 ? (
        <div className="empty">Nothing yet. Projects you upload show up here.</div>
      ) : (
        <div className="panel">
          {projects.map((p) => (
            <Row key={p.id} hub={hub} p={p} refresh={refresh} />
          ))}
        </div>
      )}
    </>
  );
}
