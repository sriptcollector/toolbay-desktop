import { useCallback, useEffect, useMemo, useState } from "react";
import { api, bundledTools, isTauri } from "./lib/api";
import { native, sourceOf, type Progress, type RunInfo } from "./lib/native";
import type { InstalledTool, Post, Tool, User } from "./lib/types";
import { NavIcon } from "./components";
import logo from "./assets/logo.png";
import { loadAppearance } from "./lib/theme";
import { HomePage, LibraryPage, RunningPage, SettingsPage, ToolDetail, ToolsPage } from "./screens";
import { BuilderSocialPage, usePresence } from "./social";
import { ProjectsPage, readyTools } from "./projects";
import { ProfilePage } from "./profile";
import { RequestsPage } from "./requests";
import { InvitePage } from "./invite";
import { projectsApi } from "./lib/projects";
import { InstallPrompt, type Wish } from "./install-prompt";

export type Page = "home" | "running" | "tools" | "community" | "library" | "settings" | "projects" | "profile" | "requests" | "invite";
export interface Route {
  page: Page;
  slug?: string;
}

export interface Hub {
  tools: Tool[];
  /** Your uploaded projects that are ready, as Tools (see projects.tsx). */
  projectTools: Tool[];
  setProjectTools: (t: Tool[]) => void;
  posts: Post[];
  library: InstalledTool[];
  running: RunInfo[];
  isRunning: (slug: string) => RunInfo | undefined;
  user: User | null;
  live: boolean;
  /** The catalog has loaded at least once (from the network or the last-run cache). */
  loaded: boolean;
  /** Per tool: an Open/Close in flight. */
  busy: Record<string, "opening" | "closing">;
  progress: Record<string, Progress>;
  /** Newer versions waiting for "Update now", by slug. */
  pendingUpdates: Record<string, string>;
  /** Slug of a tool that just finished installing (to offer a desktop shortcut). */
  justInstalled: string | null;
  setJustInstalled: (s: string | null) => void;
  go: (r: Route) => void;
  toast: (m: string) => void;
  /** Resolves true once it installed. */
  install: (t: Tool) => Promise<boolean>;
  uninstall: (t: Tool) => Promise<void>;
  open: (t: Tool) => Promise<void>;
  close: (t: Tool) => Promise<void>;
  /** Close it if open, install the waiting version, open it again. */
  update: (t: Tool) => Promise<void>;
  /** Check for a newer Toolbay and install it (progress shows in the top bar). */
  updateHub: () => Promise<"latest" | "failed" | "updating">;
  refreshPosts: () => Promise<void>;
  addPost: (p: Post) => void;
  signIn: () => void;
}

const NAV: { page: Page; label: string }[] = [
  { page: "home", label: "Home" },
  { page: "running", label: "Running now" },
  { page: "tools", label: "Tools" },
  { page: "library", label: "Library" },
  { page: "community", label: "Builder Social" },
  { page: "requests", label: "Requests" },
];

/** Tools whose slug changed on toolbay.ai, so old links and installs still find them. */
const RENAMED: Record<string, string> = { "toolbay-notes": "cereal" };

/** Toolbay updating itself, shown in the big bar at the top of the window. */
export interface HubUpdate {
  version: string;
  stage: "downloading" | "installing" | "restarting";
  received: number;
  total: number | null;
}

/** Keep Toolbay itself up to date without asking: download (with the top progress bar), install
 *  quietly, restart. Open tools are separate apps, so they keep running while the hub restarts.
 *  Resolves "latest" when there's nothing newer, "failed" when it couldn't check or install. */
let hubUpdating = false;
/** No download progress for this long (a Mac asleep mid-download, Wi-Fi gone): give up for now. */
const STALL_MS = 90_000;
async function runHubUpdate(show: (u: HubUpdate | null) => void): Promise<"latest" | "failed" | "updating"> {
  if (hubUpdating) return "updating";
  hubUpdating = true;
  let stallTimer = 0;
  // The download can't be cancelled; a stalled one is only abandoned: the bar goes away, but
  // "updating" (and the hold on closing) stay until it really ends, so two never run at once.
  let abandoned = false;
  const job: { inFlight: Promise<void> | null } = { inFlight: null };
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check({ timeout: 60_000 });
    if (!update) return "latest";
    let state: HubUpdate = { version: update.version, stage: "downloading", received: 0, total: null };
    show(state);
    // Closing the window waits for this, so Toolbay is never quit halfway through replacing itself.
    await native.hubBusy(true).catch(() => {});
    await new Promise<void>((resolve, reject) => {
      const arm = () => {
        window.clearTimeout(stallTimer);
        stallTimer = window.setTimeout(() => {
          abandoned = true;
          reject(new Error("update stalled"));
        }, STALL_MS);
      };
      arm();
      job.inFlight = update
        .downloadAndInstall((e) => {
          if (abandoned) return;
          if (e.event === "Started") state = { ...state, total: e.data.contentLength ?? null };
          else if (e.event === "Progress") state = { ...state, received: state.received + e.data.chunkLength };
          else state = { ...state, stage: "installing" };
          // Installing can take a while (a password prompt on some Macs): only downloads stall.
          if (state.stage === "downloading") arm();
          else window.clearTimeout(stallTimer);
          show(state);
        })
      job.inFlight.then(resolve, reject);
    });
    show({ ...state, stage: "restarting" });
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
    // Still here after 15 s: the restart didn't happen. Let the next check try again.
    await new Promise((r) => window.setTimeout(r, 15_000));
    show(null);
    return "failed";
  } catch {
    // Offline, stalled or no release yet: the next check tries again.
    show(null);
    return "failed";
  } finally {
    window.clearTimeout(stallTimer);
    const release = () => {
      native.hubBusy(false).catch(() => {});
      hubUpdating = false;
    };
    if (abandoned && job.inFlight) job.inFlight.then(release, release);
    else release();
  }
}

const mb = (n: number) => (n / 1048576).toFixed(1);

/** The big progress bar across the top while Toolbay or a tool updates. */
function UpdateBar({ hub, tools, progress }: { hub: HubUpdate | null; tools: Record<string, string>; progress: Record<string, Progress> }) {
  const rows: { key: string; title: string; detail: string; frac: number | null }[] = [];
  if (hub) {
    const frac = hub.stage !== "downloading" ? 1 : hub.total ? Math.min(1, hub.received / hub.total) : null;
    rows.push({
      key: "hub",
      title: hub.stage === "restarting" ? `Restarting Toolbay ${hub.version}…` : hub.stage === "installing" ? `Installing Toolbay ${hub.version}…` : `Updating Toolbay to ${hub.version}`,
      detail: hub.stage !== "downloading" ? "Almost done" : hub.total ? `${mb(hub.received)} of ${mb(hub.total)} MB · ${Math.round((frac ?? 0) * 100)}%` : `${mb(hub.received)} MB`,
      frac,
    });
  }
  for (const [slug, name] of Object.entries(tools)) {
    const p = progress[slug];
    const frac = p?.stage === "installing" ? 1 : p?.stage === "downloading" && p.total ? Math.min(1, p.received / p.total) : null;
    rows.push({
      key: slug,
      title: p?.stage === "installing" ? `Installing the new ${name}…` : `Updating ${name}`,
      detail: p?.stage === "downloading" ? (p.total ? `${mb(p.received)} of ${mb(p.total)} MB · ${Math.round((frac ?? 0) * 100)}%` : `${mb(p.received)} MB`) : p?.stage === "installing" ? "Almost done" : "Starting…",
      frac,
    });
  }
  if (!rows.length) return null;
  return (
    <div className="update-bar" role="status" aria-live="polite">
      {rows.map((r) => (
        <div className="update-bar-row" key={r.key}>
          <div className="update-bar-text">
            <span className="update-bar-title">{r.title}</span>
            <span className="update-bar-detail">{r.detail}</span>
          </div>
          <div className={`update-bar-track ${r.frac === null ? "indeterminate" : ""}`}>
            <div style={r.frac === null ? undefined : { transform: `scaleX(${r.frac})` }} />
          </div>
        </div>
      ))}
    </div>
  );
}

export default function App() {
  const [route, setRoute] = useState<Route>({ page: "home" });
  useEffect(loadAppearance, []);
  // First paint: last run's catalog, or the one bundled with the app; the live one replaces it.
  const [tools, setTools] = useState<Tool[]>(() => {
    try {
      const cached = JSON.parse(localStorage.getItem("catalog") ?? "[]");
      if (Array.isArray(cached) && cached.length) return cached;
    } catch {
      /* fall through */
    }
    return bundledTools();
  });
  const [loaded, setLoaded] = useState(tools.length > 0);
  const [busy, setBusy] = useState<Hub["busy"]>({});
  const [posts, setPosts] = useState<Post[]>([]);
  const [projectTools, setProjectTools] = useState<Tool[]>([]);
  const [library, setLibrary] = useState<InstalledTool[]>([]);
  const [running, setRunning] = useState<RunInfo[]>([]);
  const [user, setUser] = useState<User | null>(null);
  const [live, setLive] = useState(true);
  const [progress, setProgress] = useState<Record<string, Progress>>({});
  const [justInstalled, setJustInstalled] = useState<string | null>(null);
  const [pendingUpdates, setPendingUpdates] = useState<Record<string, string>>({});
  const [toastMsg, setToastMsg] = useState<string | null>(null);
  const [wish, setWish] = useState<Wish | null>(null);
  // `npm run dev`, open with ?updatebar: preview the top update bar mid-download.
  const [hubUpdate, setHubUpdate] = useState<HubUpdate | null>(() =>
    import.meta.env.DEV && location.search.includes("updatebar")
      ? { version: "0.1.14", stage: "downloading", received: 7.4 * 1048576, total: 18.2 * 1048576 }
      : null,
  );
  /** Tools updating right now (slug → name), for the top bar. */
  const [toolUpdates, setToolUpdates] = useState<Record<string, string>>({});
  const doneUpdating = useCallback((slug: string) => setToolUpdates(({ [slug]: _, ...rest }) => rest), []);

  const toast = useCallback((m: string) => {
    setToastMsg(m);
    window.setTimeout(() => setToastMsg((cur) => (cur === m ? null : cur)), 4500);
  }, []);
  const refreshLibrary = useCallback(() => {
    native.pendingUpdates().then(setPendingUpdates).catch(() => {});
    return native.library().then(setLibrary);
  }, []);
  const refreshPosts = useCallback(async () => setPosts((await api.posts()).data), []);

  // A tool renamed on toolbay.ai (same repo, new slug) stays installed under its new slug.
  useEffect(() => {
    if (!isTauri || !live) return;
    const repo = (s: string) => s.toLowerCase().replace(/^https:\/\/github\.com\//, "").replace(/\.git$/, "");
    for (const inst of library) {
      if (tools.some((t) => t.slug === inst.slug)) continue;
      const match = tools.find((t) => repo(sourceOf(t)) === repo(inst.repo) || RENAMED[inst.slug] === t.slug);
      if (match) native.rename(inst.slug, match.slug).then(refreshLibrary).catch(() => {});
    }
  }, [tools, library, live, refreshLibrary]);

  // Screens Rust queued: "Install <tool>?" from the installer's file name or toolbay://install/<slug>.
  const takeRoutes = useCallback(async () => {
    for (const r of await native.routeTake()) {
      if (r.kind === "install" && r.slug) setWish({ slug: RENAMED[r.slug] ?? r.slug, firstRun: r.firstRun, alias: r.slug });
      else if (r.kind === "project") setRoute({ page: "projects" });
      else if (r.kind === "tool" && r.slug) setRoute({ page: "tools", slug: r.slug });
    }
  }, []);

  useEffect(() => {
    // Show the last catalog instantly; refresh it now and every 10 minutes. A failed load (Wi-Fi
    // still waking up, a blip) retries after 2, 5, 15 and 30 seconds before the banner shows,
    // and again whenever the window comes back or the network returns.
    let retry = 0;
    let retryTimer = 0;
    const loadCatalog = (): Promise<void> =>
      api.tools().then((t) => {
        window.clearTimeout(retryTimer);
        if (t.live) {
          retry = 0;
          setTools(t.data);
          localStorage.setItem("catalog", JSON.stringify(t.data));
          setLive(true);
          api.posts().then((p) => p.live && setPosts(p.data));
        } else if (retry < 4) {
          retryTimer = window.setTimeout(loadCatalog, [2, 5, 15, 30][retry++] * 1000);
        } else {
          setLive(false);
        }
        setLoaded(true);
      });
    const retryNow = () => {
      retry = 0;
      loadCatalog();
    };
    loadCatalog();
    const catalogTimer = window.setInterval(retryNow, 10 * 60 * 1000);
    window.addEventListener("online", retryNow);
    const retryIfDown = () => retry > 0 && retryNow();
    window.addEventListener("focus", retryIfDown);
    api.posts().then((p) => setPosts(p.data));
    refreshLibrary();
    native.running().then(setRunning);
    const updateHub = () => runHubUpdate(setHubUpdate);
    if (isTauri) updateHub();
    const hubUpdateTimer = isTauri ? window.setInterval(updateHub, 30 * 60 * 1000) : 0;
    native.me().then(setUser);
    const subs = [
      native.onProgress((p) => {
        if (p.stage === "error") doneUpdating(p.slug);
        setProgress((cur) => {
          if (p.stage === "done" || p.stage === "error") {
            const { [p.slug]: _, ...rest } = cur;
            return rest;
          }
          return { ...cur, [p.slug]: p };
        });
      }),
      native.onRunning(setRunning),
      native.onLibrary(refreshLibrary),
      native.onUpdateStarted((u) => setToolUpdates((cur) => ({ ...cur, [u.slug]: u.name }))),
      native.onUpdateFailed((u) => {
        doneUpdating(u.slug);
        setProgress(({ [u.slug]: _, ...rest }) => rest);
      }),
      native.onUpdated((u) => {
        doneUpdating(u.slug);
        toast(`${u.name} was updated to ${u.version}.`);
        refreshLibrary();
      }),
      native.onUpdateReady((u) => {
        native.pendingUpdates().then(setPendingUpdates).catch(() => {});
        toast(`${u.name} ${u.version} is ready. It installs when you close ${u.name}, or click Update on its page.`);
      }),
      native.onAuth((u) => {
        setUser(u);
        if (u) toast(`You're signed in as ${u.name || u.handle}.`);
      }),
      native.onError(toast),
      native.onRoutePending(takeRoutes),
    ];
    takeRoutes();
    return () => {
      window.clearInterval(catalogTimer);
      window.clearInterval(hubUpdateTimer);
      window.clearTimeout(retryTimer);
      window.removeEventListener("online", retryNow);
      window.removeEventListener("focus", retryIfDown);
      subs.forEach((p) => p.then((un) => un()));
    };
  }, [refreshLibrary, toast, takeRoutes, doneUpdating]);

  // Ready uploaded projects, so Library can show and open the ones you've installed.
  useEffect(() => {
    if (!user || !isTauri) return setProjectTools([]);
    projectsApi
      .all()
      .then((ps) => setProjectTools(readyTools(ps)))
      .catch(() => {});
  }, [user]);

  // Run an Open/Close, showing it as in progress until it finishes.
  const act = useCallback(
    async (t: Tool, what: Hub["busy"][string], fn: () => Promise<void>) => {
      setBusy((b) => ({ ...b, [t.slug]: what }));
      try {
        await fn();
      } catch (e) {
        toast(String(e));
      } finally {
        window.setTimeout(() => setBusy(({ [t.slug]: _, ...rest }) => rest), what === "opening" ? 1200 : 0);
      }
    },
    [toast],
  );

  const hub: Hub = useMemo(
    () => ({
      tools,
      projectTools,
      setProjectTools,
      posts,
      library,
      running,
      isRunning: (slug) => running.find((r) => r.slug === slug),
      user,
      live,
      loaded,
      busy,
      progress,
      pendingUpdates,
      justInstalled,
      setJustInstalled,
      go: setRoute,
      toast,
      refreshPosts,
      addPost: (p) => setPosts((cur) => [p, ...cur.filter((x) => x.id !== p.id)]),
      signIn: () => native.signIn().catch((e) => toast(String(e))),
      install: async (t) => {
        if (!isTauri) return false;
        setProgress((cur) => ({ ...cur, [t.slug]: { slug: t.slug, stage: "preparing", received: 0, total: null } }));
        try {
          await native.install(t);
          await refreshLibrary();
          setJustInstalled(t.slug);
          return true;
        } catch (e) {
          toast(String(e));
          return false;
        }
      },
      uninstall: async (t) => {
        try {
          await native.uninstall(t.slug);
          await refreshLibrary();
          toast(`${t.name} was removed.`);
        } catch (e) {
          toast(String(e));
        }
      },
      open: (t) => act(t, "opening", () => native.open(t.slug)),
      close: (t) => act(t, "closing", () => native.close(t.slug)),
      updateHub: () => runHubUpdate(setHubUpdate),
      update: async (t) => {
        setProgress((cur) => ({ ...cur, [t.slug]: { slug: t.slug, stage: "preparing", received: 0, total: null } }));
        try {
          await native.updateNow(t.slug);
        } catch (e) {
          // Failed before the install began ("didn't close", "already updating"): nothing else
          // clears "Getting ready…".
          setProgress(({ [t.slug]: _, ...rest }) => rest);
          toast(String(e));
        } finally {
          await refreshLibrary();
        }
      },
    }),
    [tools, projectTools, posts, library, running, user, live, loaded, busy, progress, pendingUpdates, justInstalled, toast, refreshPosts, refreshLibrary, act],
  );
  usePresence(hub);

  let body: React.ReactNode;
  if (route.page === "tools" && route.slug) body = <ToolDetail hub={hub} slug={route.slug} />;
  else if (route.page === "tools") body = <ToolsPage hub={hub} />;
  else if (route.page === "projects") body = <ProjectsPage hub={hub} />;
  else if (route.page === "invite") body = <InvitePage hub={hub} />;
  else if (route.page === "requests") body = <RequestsPage hub={hub} id={route.slug} />;
  else if (route.page === "profile" && route.slug) body = <ProfilePage hub={hub} handle={route.slug} />;
  else if (route.page === "running") body = <RunningPage hub={hub} />;
  else if (route.page === "community") body = <BuilderSocialPage hub={hub} />;
  else if (route.page === "library") body = <LibraryPage hub={hub} />;
  else if (route.page === "settings") body = <SettingsPage hub={hub} setUser={setUser} />;
  else body = <HomePage hub={hub} />;

  return (
    <div className="frame">
      <UpdateBar hub={hubUpdate} tools={toolUpdates} progress={progress} />
      <div className="shell">
        <aside className="sidebar">
          <div className="logo">
            <img src={logo} alt="" />
            toolbay
          </div>
          {NAV.map((n) => (
            <button
              key={n.page}
              className={`nav-item ${route.page === n.page || (n.page === "library" && route.page === "projects") ? "active" : ""}`}
              onClick={() => setRoute({ page: n.page })}
            >
              <NavIcon name={n.page} />
              {n.label}
              {n.page === "library" && library.length > 0 && <span className="nav-count">{library.length}</span>}
              {n.page === "running" && running.length > 0 && (
                <span className="nav-count live">
                  <span className="dot" /> {running.length}
                </span>
              )}
            </button>
          ))}
          <div className="spacer" />
          <button
            className={`nav-item ${route.page === "invite" ? "active" : ""}`}
            onClick={() => setRoute({ page: "invite" })}
          >
            <NavIcon name="invite" />
            Invite friends
          </button>
          <button className="btn upload-cta" onClick={() => setRoute({ page: "projects" })}>
            Upload your app
          </button>
          <button
            className={`nav-item ${route.page === "settings" ? "active" : ""}`}
            onClick={() => setRoute({ page: "settings" })}
          >
            <NavIcon name="settings" />
            Settings
          </button>
          {user ? (
            <button
              className={`nav-item account ${route.page === "profile" && route.slug === user.handle ? "active" : ""}`}
              onClick={() => setRoute({ page: "profile", slug: user.handle })}
            >
              {user.avatarUrl ? (
                <img className="avatar" src={user.avatarUrl} alt="" />
              ) : (
                <div className="avatar">{(user.name || user.handle).charAt(0).toUpperCase()}</div>
              )}
              {user.name || `@${user.handle}`}
            </button>
          ) : (
            <button className="nav-item" onClick={hub.signIn}>
              <span className="avatar">?</span>
              Sign in
            </button>
          )}
        </aside>
        <main className="main">
          <div className="content" key={`${route.page}/${route.slug ?? ""}`}>
            {!live && <div className="banner">Couldn't reach toolbay.ai, so this list may be out of date.</div>}
            {body}
          </div>
        </main>
        {wish && <InstallPrompt hub={hub} wish={wish} onDone={() => setWish(null)} />}
        {toastMsg && <div className="toast">{toastMsg}</div>}
      </div>
    </div>
  );
}
