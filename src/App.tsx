import { useCallback, useEffect, useMemo, useState } from "react";
import { api, isTauri } from "./lib/api";
import { native, type Progress } from "./lib/native";
import type { InstalledTool, Post, Tool, User } from "./lib/types";
import { NavIcon } from "./components";
import logo from "./assets/logo.png";
import { CommunityPage, HomePage, LibraryPage, SettingsPage, ToolDetail, ToolsPage } from "./screens";

export type Page = "home" | "tools" | "community" | "library" | "settings";
export interface Route {
  page: Page;
  slug?: string;
}

export interface Hub {
  tools: Tool[];
  posts: Post[];
  library: InstalledTool[];
  running: string[];
  user: User | null;
  live: boolean;
  progress: Record<string, Progress>;
  /** Slug of a tool that just finished installing (to offer a desktop shortcut). */
  justInstalled: string | null;
  setJustInstalled: (s: string | null) => void;
  go: (r: Route) => void;
  toast: (m: string) => void;
  install: (t: Tool) => Promise<void>;
  uninstall: (t: Tool) => Promise<void>;
  launch: (t: Tool) => Promise<void>;
  refreshPosts: () => Promise<void>;
  signIn: () => void;
}

const NAV: { page: Page; label: string }[] = [
  { page: "home", label: "Home" },
  { page: "tools", label: "Tools" },
  { page: "library", label: "Library" },
  { page: "community", label: "Community" },
];

/** Keep Toolbay itself up to date without asking: download, install quietly, restart. */
async function updateHubQuietly() {
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) return;
    if ((await native.running()).length > 0) return; // don't restart while a tool is open
    await update.downloadAndInstall();
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch {
    // Offline or no release yet: try again next launch.
  }
}

export default function App() {
  const [route, setRoute] = useState<Route>({ page: "home" });
  const [tools, setTools] = useState<Tool[]>([]);
  const [posts, setPosts] = useState<Post[]>([]);
  const [library, setLibrary] = useState<InstalledTool[]>([]);
  const [running, setRunning] = useState<string[]>([]);
  const [user, setUser] = useState<User | null>(null);
  const [live, setLive] = useState(true);
  const [progress, setProgress] = useState<Record<string, Progress>>({});
  const [justInstalled, setJustInstalled] = useState<string | null>(null);
  const [toastMsg, setToastMsg] = useState<string | null>(null);

  const toast = useCallback((m: string) => {
    setToastMsg(m);
    window.setTimeout(() => setToastMsg((cur) => (cur === m ? null : cur)), 4500);
  }, []);
  const refreshLibrary = useCallback(() => native.library().then(setLibrary), []);
  const refreshPosts = useCallback(async () => setPosts((await api.posts()).data), []);

  useEffect(() => {
    Promise.all([api.tools(), api.posts()]).then(([t, p]) => {
      setTools(t.data);
      setPosts(p.data);
      setLive(t.live);
    });
    refreshLibrary();
    native.running().then(setRunning);
    if (isTauri) updateHubQuietly();
    native.me().then(setUser);
    const subs = [
      native.onProgress((p) =>
        setProgress((cur) => {
          if (p.stage === "done" || p.stage === "error") {
            const { [p.slug]: _, ...rest } = cur;
            return rest;
          }
          return { ...cur, [p.slug]: p };
        }),
      ),
      native.onRunning(({ slug, running }) =>
        setRunning((cur) => (running ? [...new Set([...cur, slug])] : cur.filter((s) => s !== slug))),
      ),
      native.onLibrary(refreshLibrary),
      native.onUpdated((u) => toast(`${u.name} was updated to ${u.version}.`)),
      native.onAuth((u) => {
        setUser(u);
        if (u) toast(`You're signed in as ${u.name || u.handle}.`);
      }),
      native.onError(toast),
      native.onRoute((r) => setRoute({ page: "tools", slug: r.slug })),
    ];
    return () => {
      subs.forEach((p) => p.then((un) => un()));
    };
  }, [refreshLibrary, toast]);

  const hub: Hub = useMemo(
    () => ({
      tools,
      posts,
      library,
      running,
      user,
      live,
      progress,
      justInstalled,
      setJustInstalled,
      go: setRoute,
      toast,
      refreshPosts,
      signIn: () => native.signIn().catch((e) => toast(String(e))),
      install: async (t) => {
        if (!isTauri) return;
        setProgress((cur) => ({ ...cur, [t.slug]: { slug: t.slug, stage: "preparing", received: 0, total: null } }));
        try {
          await native.install(t);
          await refreshLibrary();
          setJustInstalled(t.slug);
        } catch (e) {
          toast(String(e));
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
      launch: async (t) => {
        try {
          await native.launch(t.slug);
        } catch (e) {
          toast(String(e));
        }
      },
    }),
    [tools, posts, library, running, user, live, progress, justInstalled, toast, refreshPosts, refreshLibrary],
  );

  let body: React.ReactNode;
  if (route.page === "tools" && route.slug) body = <ToolDetail hub={hub} slug={route.slug} />;
  else if (route.page === "tools") body = <ToolsPage hub={hub} />;
  else if (route.page === "community") body = <CommunityPage hub={hub} />;
  else if (route.page === "library") body = <LibraryPage hub={hub} />;
  else if (route.page === "settings") body = <SettingsPage hub={hub} setUser={setUser} />;
  else body = <HomePage hub={hub} />;

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="logo">
          <img src={logo} alt="" />
          toolbay
        </div>
        {NAV.map((n) => (
          <button
            key={n.page}
            className={`nav-item ${route.page === n.page ? "active" : ""}`}
            onClick={() => setRoute({ page: n.page })}
          >
            <NavIcon name={n.page} />
            {n.label}
            {n.page === "library" && library.length > 0 && <span className="nav-count">{library.length}</span>}
          </button>
        ))}
        <div className="spacer" />
        <button
          className={`nav-item ${route.page === "settings" ? "active" : ""}`}
          onClick={() => setRoute({ page: "settings" })}
        >
          <NavIcon name="settings" />
          Settings
        </button>
        {user ? (
          <div className="account">
            <div className="avatar">{(user.name || user.handle).charAt(0).toUpperCase()}</div>
            {user.name || `@${user.handle}`}
          </div>
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
      {toastMsg && <div className="toast">{toastMsg}</div>}
    </div>
  );
}
