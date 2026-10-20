import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, isTauri } from "./lib/api";
import { native, type Progress } from "./lib/native";
import type { InstalledTool, Post, Skill, Tool, User } from "./lib/types";
import { NavIcon } from "./components";
import logo from "./assets/logo.png";
import {
  CommunityPage,
  HomePage,
  LibraryPage,
  SettingsPage,
  SkillDetail,
  SkillsPage,
  ToolDetail,
  ToolsPage,
} from "./screens";

export type Page = "home" | "tools" | "skills" | "community" | "library" | "settings";
export interface Route {
  page: Page;
  slug?: string;
}

export interface Hub {
  tools: Tool[];
  skills: Skill[];
  posts: Post[];
  library: InstalledTool[];
  user: User | null;
  live: boolean;
  progress: Record<string, Progress>;
  go: (r: Route) => void;
  toast: (m: string) => void;
  install: (t: Tool) => Promise<void>;
  uninstall: (slug: string) => Promise<void>;
  launch: (slug: string) => Promise<void>;
  refreshPosts: () => Promise<void>;
  signIn: () => void;
}

const NAV: { page: Page; label: string }[] = [
  { page: "home", label: "Home" },
  { page: "tools", label: "Tools" },
  { page: "skills", label: "Skills" },
  { page: "community", label: "Community" },
  { page: "library", label: "Library" },
];

export default function App() {
  const [route, setRoute] = useState<Route>({ page: "home" });
  const [tools, setTools] = useState<Tool[]>([]);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [posts, setPosts] = useState<Post[]>([]);
  const [library, setLibrary] = useState<InstalledTool[]>([]);
  const [user, setUser] = useState<User | null>(null);
  const [live, setLive] = useState(true);
  const [progress, setProgress] = useState<Record<string, Progress>>({});
  const [toastMsg, setToastMsg] = useState<string | null>(null);

  const toast = useCallback((m: string) => {
    setToastMsg(m);
    window.setTimeout(() => setToastMsg((cur) => (cur === m ? null : cur)), 3500);
  }, []);

  const refreshLibrary = useCallback(() => native.library().then(setLibrary), []);
  const refreshPosts = useCallback(async () => {
    const r = await api.posts();
    setPosts(r.data);
  }, []);

  useEffect(() => {
    Promise.all([api.tools(), api.skills(), api.posts()]).then(([t, s, p]) => {
      setTools(t.data);
      setSkills(s.data);
      setPosts(p.data);
      setLive(t.live && s.live);
    });
    refreshLibrary();
    native.me().then(setUser);
    native.settings().then((s) => {
      document.documentElement.dataset.theme = s.theme === "dark" ? "dark" : "light";
    });
    const subs = [
      native.onProgress((p) => setProgress((cur) => ({ ...cur, [p.slug]: p }))),
      native.onAuth((u) => {
        setUser(u);
        if (u) toast(`Signed in as ${u.name || u.handle}`);
      }),
      native.onAuthError(toast),
      isTauri
        ? listen<{ kind: string; slug: string }>("open-route", (e) =>
            setRoute({ page: e.payload.kind === "skill" ? "skills" : "tools", slug: e.payload.slug }),
          )
        : Promise.resolve(() => {}),
    ];
    return () => {
      subs.forEach((p) => p.then((un) => un()));
    };
  }, [refreshLibrary, toast]);

  const hub: Hub = useMemo(
    () => ({
      tools,
      skills,
      posts,
      library,
      user,
      live,
      progress,
      go: setRoute,
      toast,
      refreshPosts,
      signIn: () => native.signIn().catch((e) => toast(String(e))),
      install: async (t) => {
        if (!isTauri) return toast("Install works in the desktop app.");
        setProgress((cur) => ({ ...cur, [t.slug]: { slug: t.slug, stage: "downloading", received: 0, total: null } }));
        try {
          await native.install(t);
          await refreshLibrary();
          toast(`${t.name} installed`);
        } catch (e) {
          toast(String(e));
        } finally {
          setProgress((cur) => {
            const { [t.slug]: _, ...rest } = cur;
            return rest;
          });
        }
      },
      uninstall: async (slug) => {
        try {
          await native.uninstall(slug);
          await refreshLibrary();
          toast("Uninstalled");
        } catch (e) {
          toast(String(e));
        }
      },
      launch: async (slug) => {
        try {
          await native.launch(slug);
        } catch (e) {
          toast(String(e));
        }
      },
    }),
    [tools, skills, posts, library, user, live, progress, toast, refreshPosts, refreshLibrary],
  );

  let body: React.ReactNode;
  if (route.page === "tools" && route.slug) body = <ToolDetail hub={hub} slug={route.slug} />;
  else if (route.page === "skills" && route.slug) body = <SkillDetail hub={hub} slug={route.slug} />;
  else if (route.page === "tools") body = <ToolsPage hub={hub} />;
  else if (route.page === "skills") body = <SkillsPage hub={hub} />;
  else if (route.page === "community") body = <CommunityPage hub={hub} />;
  else if (route.page === "library") body = <LibraryPage hub={hub} />;
  else if (route.page === "settings") body = <SettingsPage hub={hub} setUser={setUser} />;
  else body = <HomePage hub={hub} />;

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="logo">
          <div className="logo-mark">
            <img src={logo} alt="" />
          </div>
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
        <div className="sidebar-spacer" />
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
        <div className="card">
          <div className="content" key={`${route.page}/${route.slug ?? ""}`}>
            {!live && (
              <div className="banner">Showing sample data. The Toolbay catalog API isn't reachable yet.</div>
            )}
            {body}
          </div>
        </div>
      </main>
      {toastMsg && <div className="toast">{toastMsg}</div>}
    </div>
  );
}
