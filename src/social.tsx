// Builder Social: everyone on Toolbay with GitHub connected, who's online and what they're
// building right now, friend requests, three builders who fit with you like puzzle pieces, and
// the feed. Needs a Toolbay account with GitHub connected (the server checks too).
// API: /api/v1/social/* on toolbay.ai (src/lib/builder-social.ts in the site repo).
import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Hub } from "./App";
import { callApi, isTauri } from "./lib/api";
import { native } from "./lib/native";
import { CommunityPage } from "./screens";

type ActivityMode = "off" | "friends" | "everyone";
interface Settings {
  onboarded: boolean;
  autoProfile: boolean;
  autoPost: boolean;
  shareActivity: ActivityMode;
  skills: string | null;
  tags: string[];
}
type Me = { github: false; connectUrl: string } | { github: true; githubUsername: string; settings: Settings; drafts: { body: string; linkUrl: string | null }[] };

export interface Builder {
  handle: string;
  name: string;
  avatarUrl: string | null;
  skills: string | null;
  tags: string[];
  online: boolean;
  lastSeenAt: string | null;
  activity: string | null;
  apps: number;
  following: boolean;
  friend: "none" | "friends" | "incoming" | "outgoing";
  reason?: string;
}
interface People {
  builders: Builder[];
  recommended: Builder[];
  requests: Builder[];
}

const open = (url: string) => (isTauri ? openUrl(url) : window.open(url, "_blank"));
const ACTIVITY_LABEL: [ActivityMode, string][] = [
  ["off", "Off"],
  ["friends", "Friends"],
  ["everyone", "Everyone"],
];

/** `npm run dev`, open with ?social (or ?social=onboard): design the page without the desktop app. */
const demoParam = import.meta.env.DEV && !isTauri ? new URLSearchParams(location.search).get("social") : null;
const DEMO = demoParam === null ? null : demoParam || "home";
const demoSettings: Settings = {
  onboarded: DEMO !== "onboard",
  autoProfile: true,
  autoPost: true,
  shareActivity: "off",
  skills: "Builds local AI desktop apps in Rust and TypeScript; strong on voice tools",
  tags: ["TypeScript", "Rust", "AI", "desktop apps"],
};
const demoPeople: People = (() => {
  const b = (handle: string, name: string, skills: string, tags: string[], online: boolean, activity: string | null, extra: Partial<Builder> = {}): Builder => ({
    handle, name, avatarUrl: null, skills, tags, online, lastSeenAt: new Date(Date.now() - (online ? 30e3 : 5 * 3600e3)).toISOString(), activity, apps: 1, following: false, friend: "none", ...extra,
  });
  const all = [
    b("maya", "Maya Chen", "Designs and builds iOS apps in Swift; strong on motion and onboarding", ["Swift", "mobile apps", "design"], true, "Designing in Figma"),
    b("dev_raj", "Raj Patel", "Builds Postgres-backed APIs in Go; ships billing and auth backends", ["Go", "backends", "TypeScript"], true, "Coding in VS Code · ledger-api"),
    b("lena", "Lena Ortiz", "Makes games in Godot and C#", ["C#", "games"], false, null, { friend: "incoming" }),
    b("sam", "Sam Okafor", "Automates scraping and workflows in Python", ["Python", "automation"], false, null, { following: true }),
  ];
  return {
    builders: all,
    recommended: all.slice(0, 3).filter((x) => !x.following).map((x, i) => ({ ...x, reason: ["Strong in Swift and design; you both work with AI", "Fills in Go and backends for you", "Fills in games for you"][i] })),
    requests: all.filter((x) => x.friend === "incoming"),
  };
})();

/** Settings changed somewhere (Settings page, onboarding): the heartbeat re-reads them. */
const SETTINGS_EVENT = "social-settings";
const getMe = () => callApi<Me>("GET", "/api/v1/social/me");

function lastSeen(iso: string | null): string {
  if (!iso) return "";
  const m = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (m < 60) return `${Math.max(1, m)}m ago`;
  const h = Math.round(m / 60);
  return h < 48 ? `${h}h ago` : `${Math.round(h / 24)}d ago`;
}

function Avatar({ b, size = 32 }: { b: Pick<Builder, "avatarUrl" | "name" | "handle" | "online">; size?: number }) {
  return (
    <span className="social-avatar" style={{ width: size, height: size }}>
      {b.avatarUrl ? <img src={b.avatarUrl} alt="" /> : <span>{(b.name || b.handle).charAt(0).toUpperCase()}</span>}
      {b.online && <i className="online-dot" title="Online now" />}
    </span>
  );
}

// ---------------- Page ----------------

export function BuilderSocialPage({ hub }: { hub: Hub }) {
  if (DEMO) {
    const drafts = [
      { body: "I built Toolbay Flow: hold a key, speak, and clean text appears where your cursor is", linkUrl: "https://toolbay.ai/tools/toolbay-flow" },
      { body: "Working on bedrock (TypeScript): Obsidian-compatible notes with sync built in", linkUrl: "https://github.com/sriptcollector/bedrock" },
    ];
    return demoSettings.onboarded ? (
      <SocialHome hub={hub} settings={demoSettings} />
    ) : (
      <Onboarding me={{ github: true, githubUsername: "demo", settings: demoSettings, drafts }} onDone={() => {}} hub={hub} />
    );
  }
  return <LiveSocialPage hub={hub} />;
}

function LiveSocialPage({ hub }: { hub: Hub }) {
  const [me, setMe] = useState<Me | null | "error">(null);
  const load = useCallback(() => {
    if (!hub.user) return;
    getMe()
      .then(setMe)
      // A blip keeps what's on screen (an onboarding in progress isn't thrown away).
      .catch(() => setMe((cur) => (cur && cur !== "error" ? cur : "error")));
  }, [hub.user]);
  useEffect(load, [load]);
  // Back from connecting GitHub in the browser: check again (only while waiting for that).
  const waitingForGithub = !!me && me !== "error" && !me.github;
  useEffect(() => {
    if (!waitingForGithub) return;
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, [load, waitingForGithub]);

  if (!hub.user) {
    return (
      <Gate hub={hub} title="Builder Social" body="Meet the people building on Toolbay. Sign in with your Toolbay account, then connect GitHub.">
        <button className="btn primary" onClick={hub.signIn}>
          Sign in
        </button>
      </Gate>
    );
  }
  if (me === null) return <div className="empty">Loading…</div>;
  if (me === "error") {
    return (
      <Gate hub={hub} title="Builder Social" body="Couldn't reach toolbay.ai. Check your internet connection.">
        <button className="btn" onClick={load}>
          Try again
        </button>
      </Gate>
    );
  }
  if (!me.github) {
    return (
      <Gate
        hub={hub}
        title="Connect GitHub to join Builder Social"
        body="Builder Social is for people who build. Connect GitHub to your Toolbay account and your profile fills in from your work: what you're good at, your projects, and who you'd build well with."
      >
        <button className="btn primary" onClick={() => open(me.connectUrl)}>
          Connect GitHub
        </button>
        <button className="btn" onClick={load}>
          I've connected it
        </button>
      </Gate>
    );
  }
  if (!me.settings.onboarded) return <Onboarding me={me} onDone={load} hub={hub} />;
  return <SocialHome hub={hub} settings={me.settings} />;
}

/** What's needed to join, with the feed underneath: anyone can still read and post. */
function Gate({ hub, title, body, children }: { hub: Hub; title: string; body: string; children: React.ReactNode }) {
  return (
    <>
      <div className="social-gate">
        <h1>{title}</h1>
        <p className="lede">{body}</p>
        <div className="row" style={{ gap: 8 }}>
          {children}
        </div>
      </div>
      <h2>Feed</h2>
      <CommunityPage hub={hub} embedded />
    </>
  );
}

// ---------------- Onboarding ----------------

function Onboarding({ me, onDone, hub }: { me: Extract<Me, { github: true }>; onDone: () => void; hub: Hub }) {
  const [autoProfile, setAutoProfile] = useState(true);
  const [autoPost, setAutoPost] = useState(true);
  const [share, setShare] = useState<ActivityMode>("off");
  // Unticked drafts, by link (or text): a refreshed list keeps ticks, new drafts start ticked.
  const [unpicked, setUnpicked] = useState<Set<string>>(() => new Set());
  const keyOf = (d: { body: string; linkUrl: string | null }) => d.linkUrl ?? d.body;
  const [saving, setSaving] = useState(false);

  const finish = async () => {
    setSaving(true);
    try {
      const r = await callApi<{ posted: number }>("POST", "/api/v1/social/onboard", {
        autoProfile,
        autoPost,
        shareActivity: share,
        post: me.drafts.map(keyOf).filter((k) => !unpicked.has(k)),
      });
      if (r.posted) {
        hub.toast(`Posted ${r.posted} ${r.posted === 1 ? "project" : "projects"} for you.`);
        hub.refreshPosts();
      }
      window.dispatchEvent(new Event(SETTINGS_EVENT));
      onDone();
    } catch (e) {
      hub.toast(String((e as Error).message ?? e));
      setSaving(false);
    }
  };

  return (
    <div className="social-onboarding">
      <h1>Welcome to Builder Social</h1>
      <p className="lede">
        Builders fit together like puzzle pieces. We'll show you who's building right now and suggest people whose skills fill in yours.
      </p>

      <div className="panel social-preview">
        <div className="small muted">What people will see you're good at</div>
        <div className="social-skills">{me.settings.skills || "We'll write this from your builds once you've shipped or pushed something public."}</div>
      </div>

      <Toggle
        title="Keep my profile updated from my work"
        body="Your “good at” line refreshes every day from your Toolbay apps and public GitHub repos."
        on={autoProfile}
        set={setAutoProfile}
      />
      <Toggle
        title="Post about my work for me"
        body="Posts your projects now, then one short build log on days you push code, ship a release or publish an app. Never on days with nothing new."
        on={autoPost}
        set={setAutoPost}
      />
      {autoPost && me.drafts.length > 0 && (
        <div className="panel" style={{ marginBottom: 12 }}>
          {me.drafts.map((d) => (
            <label className="list-row clickable" key={keyOf(d)}>
              <input
                type="checkbox"
                checked={!unpicked.has(keyOf(d))}
                onChange={(e) =>
                  setUnpicked((s) => {
                    const n = new Set(s);
                    if (e.target.checked) n.delete(keyOf(d));
                    else n.add(keyOf(d));
                    return n;
                  })
                }
              />
              <div className="grow">
                <div>{d.body}</div>
                {d.linkUrl && <div className="small muted mono">{d.linkUrl.replace(/^https:\/\//, "")}</div>}
              </div>
            </label>
          ))}
        </div>
      )}
      <div className="settings-row">
        <div className="grow">
          <div className="title">Show what I'm working on</div>
          <div className="small muted">
            While Toolbay is open: the builder app in front (like “Coding in VS Code · my-project” or “Designing in Figma”). Other apps are never shown.
          </div>
        </div>
        <Seg value={share} set={setShare} />
      </div>
      <div className="row" style={{ marginTop: 16 }}>
        <button className="btn primary" disabled={saving} onClick={finish}>
          {saving ? "Setting up…" : "Join Builder Social"}
        </button>
      </div>
    </div>
  );
}

function Toggle({ title, body, on, set }: { title: string; body: string; on: boolean; set: (v: boolean) => void }) {
  return (
    <div className="settings-row">
      <div className="grow">
        <div className="title">{title}</div>
        <div className="small muted">{body}</div>
      </div>
      <div className="seg">
        <button className={on ? "on" : ""} onClick={() => set(true)}>
          On
        </button>
        <button className={!on ? "on" : ""} onClick={() => set(false)}>
          Off
        </button>
      </div>
    </div>
  );
}

function Seg({ value, set }: { value: ActivityMode; set: (v: ActivityMode) => void }) {
  return (
    <div className="seg">
      {ACTIVITY_LABEL.map(([v, label]) => (
        <button key={v} className={value === v ? "on" : ""} onClick={() => set(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}

// ---------------- Home ----------------

function SocialHome({ hub, settings }: { hub: Hub; settings: Settings }) {
  const [people, setPeople] = useState<People | null>(null);
  const [tab, setTab] = useState<"people" | "feed">("people");
  const [q, setQ] = useState("");

  const refresh = useCallback(() => {
    if (DEMO) return setPeople((p) => p ?? demoPeople);
    callApi<People>("GET", "/api/v1/social/builders")
      .then(setPeople)
      .catch(() => {});
  }, []);
  // Live: refresh every 20 seconds while this page is open.
  useEffect(() => {
    refresh();
    const t = window.setInterval(refresh, 20_000);
    return () => window.clearInterval(t);
  }, [refresh]);

  const patch = (handle: string, change: Partial<Builder>) =>
    setPeople((p) => {
      if (!p) return p;
      const upd = (l: Builder[]) => l.map((b) => (b.handle === handle ? { ...b, ...change } : b));
      return { builders: upd(p.builders), recommended: upd(p.recommended), requests: upd(p.requests).filter((b) => b.friend === "incoming") };
    });
  const follow = async (b: Builder) => {
    patch(b.handle, { following: !b.following });
    try {
      const r = await callApi<{ following: boolean }>("POST", "/api/v1/social/follow", { handle: b.handle, follow: !b.following });
      patch(b.handle, { following: r.following });
    } catch (e) {
      patch(b.handle, { following: b.following });
      hub.toast((e as Error).message);
    }
  };
  const friend = async (b: Builder, action: "request" | "accept" | "decline" | "remove") => {
    try {
      const r = await callApi<{ friend: Builder["friend"] }>("POST", "/api/v1/social/friends", { handle: b.handle, action });
      patch(b.handle, { friend: r.friend });
      if (action === "accept") hub.toast(`You and ${b.name} are friends now.`);
    } catch (e) {
      hub.toast((e as Error).message);
    }
  };

  const me = hub.user?.handle;
  const others = people?.builders.filter((b) => b.handle !== me) ?? [];
  const live = others.filter((b) => b.online);
  const term = q.trim().toLowerCase();
  const list = term
    ? others.filter((b) => `${b.name} ${b.handle} ${b.skills ?? ""} ${b.tags.join(" ")}`.toLowerCase().includes(term))
    : others;
  const actions = { follow, friend, go: (h: string) => hub.go({ page: "profile", slug: h }) };

  return (
    <>
      <div className="row" style={{ justifyContent: "space-between", alignItems: "baseline" }}>
        <h1>Builder Social</h1>
        <span className="small muted">
          {people ? `${live.length} online · ${others.length} builders` : ""}
        </span>
      </div>
      {settings.skills && (
        <p className="lede">
          You: {settings.skills}
          {settings.shareActivity === "off" && (
            <>
              {" "}
              <button className="link" onClick={() => hub.go({ page: "settings" })}>
                Share what you're working on
              </button>
            </>
          )}
        </p>
      )}

      {live.length > 0 && (
        <div className="live-strip">
          {live.map((b) => (
            <button key={b.handle} className="live-chip" onClick={() => actions.go(b.handle)} title={b.skills ?? ""}>
              <Avatar b={b} size={28} />
              <span className="live-text">
                <span className="live-name">{b.name}</span>
                <span className="small muted">{b.activity ?? "Online"}</span>
              </span>
            </button>
          ))}
        </div>
      )}

      {!!people?.requests.length && (
        <>
          <h2>Friend requests</h2>
          <div className="panel">
            {people.requests.map((b) => (
              <BuilderRow key={b.handle} b={b} {...actions} />
            ))}
          </div>
        </>
      )}

      {!!people?.recommended.length && (
        <>
          <h2>Builders who fit with you</h2>
          <div className="rec-grid">
            {people.recommended.map((b) => (
              <div className="panel rec-card" key={b.handle}>
                <button className="rec-head" onClick={() => actions.go(b.handle)}>
                  <Avatar b={b} size={40} />
                  <span className="grow">
                    <span className="title">{b.name}</span>
                    <span className="small muted">@{b.handle}</span>
                  </span>
                </button>
                <div className="small rec-reason">{b.reason}</div>
                {b.skills && <div className="small muted rec-skills">{b.skills}</div>}
                <div className="row" style={{ gap: 6, marginTop: "auto" }}>
                  <button className={`btn small ${b.following ? "" : "primary"}`} onClick={() => follow(b)}>
                    {b.following ? "Following" : "Follow"}
                  </button>
                  <FriendButton b={b} friend={friend} />
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      <div className="row social-tabs">
        <div className="seg">
          <button className={tab === "people" ? "on" : ""} onClick={() => setTab("people")}>
            Everyone
          </button>
          <button className={tab === "feed" ? "on" : ""} onClick={() => setTab("feed")}>
            Feed
          </button>
        </div>
        {tab === "people" && <input className="input social-search" placeholder="Search by name or skill" value={q} onChange={(e) => setQ(e.target.value)} />}
      </div>

      {tab === "feed" ? (
        <CommunityPage hub={hub} embedded />
      ) : !people ? (
        <div className="empty">Loading…</div>
      ) : list.length === 0 ? (
        <div className="empty">{term ? "No one matches that yet." : "You're one of the first here. Invite a friend who builds."}</div>
      ) : (
        <div className="panel">
          {list.map((b) => (
            <BuilderRow key={b.handle} b={b} {...actions} />
          ))}
        </div>
      )}
    </>
  );
}

function FriendButton({ b, friend }: { b: Builder; friend: (b: Builder, a: "request" | "accept" | "decline" | "remove") => void }) {
  if (b.friend === "friends") return <span className="chip success">Friends</span>;
  if (b.friend === "outgoing")
    return (
      <button className="btn small" onClick={() => friend(b, "remove")} title="Cancel request">
        Requested
      </button>
    );
  if (b.friend === "incoming")
    return (
      <>
        <button className="btn small primary" onClick={() => friend(b, "accept")}>
          Accept
        </button>
        <button className="btn small" onClick={() => friend(b, "decline")}>
          Decline
        </button>
      </>
    );
  return (
    <button className="btn small" onClick={() => friend(b, "request")}>
      Add friend
    </button>
  );
}

function BuilderRow({
  b,
  follow,
  friend,
  go,
}: {
  b: Builder;
  follow: (b: Builder) => void;
  friend: (b: Builder, a: "request" | "accept" | "decline" | "remove") => void;
  go: (h: string) => void;
}) {
  return (
    <div className="list-row builder-row">
      <button className="builder-main" onClick={() => go(b.handle)}>
        <Avatar b={b} />
        <span className="grow">
          <span className="title">
            {b.name} <span className="small muted">@{b.handle}</span>
          </span>
          <span className="small muted builder-skills">{b.skills || b.tags.slice(0, 4).join(" · ") || "New builder"}</span>
          <span className="small builder-status">
            {b.online ? <span className="live-word">{b.activity ?? "Online now"}</span> : b.lastSeenAt ? `Seen ${lastSeen(b.lastSeenAt)}` : ""}
          </span>
        </span>
      </button>
      <div className="row" style={{ gap: 6 }}>
        <button className="btn small" onClick={() => follow(b)}>
          {b.following ? "Following" : "Follow"}
        </button>
        <FriendButton b={b} friend={friend} />
      </div>
    </div>
  );
}

// ---------------- Settings ----------------

/** Settings page section: what Builder Social does automatically. */
export function BuilderSocialSettings({ hub }: { hub: Hub }) {
  const [s, setS] = useState<Settings | null>(null);
  useEffect(() => {
    if (!hub.user || !isTauri) return;
    getMe()
      .then((m) => m.github && m.settings.onboarded && setS(m.settings))
      .catch(() => {});
  }, [hub.user]);
  if (!s) return null;
  const save = async (change: Partial<Settings>) => {
    const before = s;
    setS({ ...s, ...change });
    try {
      const r = await callApi<{ settings: Settings }>("PATCH", "/api/v1/social/me", change);
      setS(r.settings);
      window.dispatchEvent(new Event(SETTINGS_EVENT));
    } catch (e) {
      setS(before);
      hub.toast((e as Error).message);
    }
  };
  return (
    <>
      <Toggle
        title="Builder Social: keep my profile updated"
        body="Your “good at” line refreshes daily from your Toolbay apps and public GitHub repos."
        on={s.autoProfile}
        set={(v) => save({ autoProfile: v })}
      />
      <Toggle
        title="Builder Social: post about my work"
        body="One short build log on days you push code, ship a release or publish an app."
        on={s.autoPost}
        set={(v) => save({ autoPost: v })}
      />
      <div className="settings-row">
        <div className="grow">
          <div className="title">Builder Social: show what I'm working on</div>
          <div className="small muted">The builder app in front while Toolbay is open. Other apps are never shown.</div>
        </div>
        <Seg value={s.shareActivity} set={(v) => save({ shareActivity: v })} />
      </div>
    </>
  );
}

// ---------------- Presence ----------------

/** One line for the app in front; `toolNames` (installed Toolbay tools) are recognised too. */
function describe(fg: { label: string | null; project: string | null; app: string | null }, toolNames: string[]): string | null {
  if (fg.label) return fg.project ? `${fg.label} · ${fg.project}` : fg.label;
  const app = fg.app?.toLowerCase().replace(/\.exe$/, "") ?? "";
  const tool = toolNames.find((n) => app && (app === n.toLowerCase() || app === n.toLowerCase().replace(/\s+/g, "")));
  return tool ? `Using ${tool}` : null;
}

/**
 * While the hub is open (for people who joined Builder Social): a heartbeat every minute marks
 * you online, with what you're working on when you share it. Stops when the hub closes.
 */
export function usePresence(hub: Hub) {
  const signedIn = !!hub.user;
  const names = hub.library.map((t) => t.name).join("\n");
  useEffect(() => {
    if (!signedIn || !isTauri) return;
    let stopped = false;
    let share: ActivityMode = "off";
    let member = false;
    // Settings are read once, then again only after a failure (offline at launch, GitHub
    // disconnected) or when they change, so a bad start never leaves presence off for the session.
    let known = false;
    let checkedAt = 0;
    const readSettings = () => {
      checkedAt = Date.now();
      return getMe()
        .then((m) => {
          member = m.github && m.settings.onboarded;
          share = m.github ? m.settings.shareActivity : "off";
          known = true;
        })
        .catch(() => {
          known = false;
        });
    };
    const beat = async () => {
      if (stopped) return;
      // Not joined (or unknown): look again every 10 minutes at most, not every beat.
      if ((!known || !member) && Date.now() - checkedAt > 10 * 60_000) await readSettings();
      if (stopped || !member) return;
      let activity: string | null = null;
      if (share !== "off") {
        const fg = await native.foreground().catch(() => null);
        activity = fg ? describe(fg, names.split("\n").filter(Boolean)) : null;
        if (!activity && document.hasFocus()) activity = "Browsing Toolbay";
      }
      try {
        const r = await callApi<{ shareActivity?: ActivityMode }>("POST", "/api/v1/social/presence", { activity });
        if (r.shareActivity) share = r.shareActivity;
      } catch {
        // Signed out or GitHub disconnected (or offline): stop until the settings say otherwise.
        member = false;
        known = false;
        checkedAt = 0;
      }
    };
    const onSettings = () => readSettings().then(beat);
    readSettings().then(beat);
    const t = window.setInterval(beat, 60_000);
    window.addEventListener(SETTINGS_EVENT, onSettings);
    return () => {
      stopped = true;
      window.clearInterval(t);
      window.removeEventListener(SETTINGS_EVENT, onSettings);
    };
  }, [signedIn, names]);
}
