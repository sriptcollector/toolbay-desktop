import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Hub } from "./App";
import { api, isTauri } from "./lib/api";
import { native } from "./lib/native";
import { setAppearance, useAppearance, type Appearance } from "./lib/theme";
import type { ForkSummary, Review, Tool, User } from "./lib/types";
import { ForkedFrom, Icon, Inspired, PrivacyChip, ProfileLink, Stars, timeAgo } from "./components";
import { DropCard, DropEmailsSetting, ToolSpace } from "./spaces";
import { shareTool } from "./invite";
import { RunStatus, ToolControls } from "./controls";

const open = (url: string) => (isTauri ? openUrl(url) : window.open(url, "_blank"));

function ShortcutOffer({ hub, tool }: { hub: Hub; tool: Tool }) {
  if (hub.justInstalled !== tool.slug) return null;
  const add = async () => {
    try {
      await native.shortcut(tool.slug);
      hub.toast(`Added ${tool.name} to your desktop.`);
    } catch (e) {
      hub.toast(String(e));
    }
    hub.setJustInstalled(null);
  };
  return (
    <div className="notice">
      <div className="grow">
        <div className="title">{tool.name} is installed.</div>
        <div className="small muted">Add a shortcut to your desktop so you can open it in one click?</div>
      </div>
      <button className="btn" onClick={() => hub.setJustInstalled(null)}>
        No thanks
      </button>
      <button className="btn primary" onClick={add}>
        Add shortcut
      </button>
    </div>
  );
}

function Hero({ hub, tool, onTitle }: { hub: Hub; tool: Tool; onTitle?: () => void }) {
  return (
    <div className="hero">
      <Icon tool={tool} className="hero-icon" />
      <div className="hero-body">
        <h1 className="hero-title" onClick={onTitle} style={onTitle ? { cursor: "pointer" } : undefined}>
          {tool.name}
        </h1>
        <div className="hero-tagline">{tool.tagline}</div>
        <div className="hero-meta">
          {tool.forkedFrom && <ForkedFrom hub={hub} from={tool.forkedFrom} />}
          <Inspired tool={tool} />
          {tool.status === "early_access" && <span className="chip ok">Early access</span>}
          <span className="chip">Free</span>
          <span className="chip">Open source · {tool.license}</span>
          <PrivacyChip privacy={tool.privacy} />
        </div>
      </div>
      <div className="hero-action">
        <ToolControls hub={hub} tool={tool} size="hero" />
        <div className="hero-secondary">
          <button className="btn small" onClick={() => shareTool(hub, tool.slug, tool.name)}>
            Share
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------- Home ----------------

export function HomePage({ hub }: { hub: Hub }) {
  const featured = hub.tools[0];
  if (!hub.loaded && !featured) return <div className="empty">Loading tools…</div>;
  const first = hub.user?.name?.split(" ")[0];
  return (
    <>
      <h1>{first ? `Welcome back, ${first}.` : "Welcome to Toolbay."}</h1>
      <p className="lede">First we reverse engineered Wispr Flow and made it free. Then we saw Obsidian was charging, so we made that free too. Now we're making Granola free. Stop paying for dumb shit.</p>
      {hub.running.length > 0 && (
        <>
          <h2 style={{ marginTop: 0 }}>Running now</h2>
          <RunningList hub={hub} />
          <h2>Featured</h2>
        </>
      )}
      {featured && (
        <>
          <Hero hub={hub} tool={featured} onTitle={() => hub.go({ page: "tools", slug: featured.slug })} />
          <ShortcutOffer hub={hub} tool={featured} />
        </>
      )}
      <DropCard />
      <h2>All tools</h2>
      <ToolGrid hub={hub} />
    </>
  );
}

function ToolGrid({ hub, tools }: { hub: Hub; tools?: Tool[] }) {
  const list = tools ?? hub.tools;
  return (
    <div className="grid">
      {list.map((t) => {
        const installed = hub.library.some((l) => l.slug === t.slug);
        return (
          <button key={t.slug} className="tile" onClick={() => hub.go({ page: "tools", slug: t.slug })}>
            <div className="tile-art">
              <Icon tool={t} />
              {hub.isRunning(t.slug) ? (
                <span className="tile-badge status live">
                  <span className="dot" />
                  Running
                </span>
              ) : installed ? (
                <span className="tile-badge status ok">Installed</span>
              ) : null}
            </div>
            <div className="tile-body">
              <div className="tile-name">{t.name}</div>
              <div className="tile-sub">
                {t.status === "early_access" ? (
                  "Early access"
                ) : t.forkedFrom ? (
                  <ForkedFrom hub={hub} from={t.forkedFrom} />
                ) : t.inspiredBy ? (
                  <Inspired tool={t} />
                ) : (
                  t.category
                )}
              </div>
            </div>
          </button>
        );
      })}
      {!tools && (
        <div className="tile placeholder">
          <div className="tile-art">More free tools are on the way</div>
          <div className="tile-body">
            <div className="tile-sub">New tools show up here automatically.</div>
          </div>
        </div>
      )}
    </div>
  );
}

export function ToolsPage({ hub }: { hub: Hub }) {
  return (
    <>
      <h1>Tools</h1>
      <p className="lede">Stop paying for dumb shit. Every app here is free, shows its source code and has a privacy check.</p>
      <ToolGrid hub={hub} />
    </>
  );
}

// ---------------- Tool page ----------------

/** "Windows · Mac" for platforms where Toolbay's automatic test installed and opened the app.
 *  (Tray and many Electron apps ignore a polite close and get force-closed; that's still a pass.) */
function testedOn(tool: Tool) {
  const ok = (tool.compatibility ?? []).filter((c) => c.installs && c.opens);
  const names = [
    ok.some((c) => c.platform.startsWith("windows")) && "Windows",
    ok.some((c) => c.platform.startsWith("darwin")) && "Mac",
  ].filter(Boolean);
  return names.join(" · ");
}

export function ToolDetail({ hub, slug }: { hub: Hub; slug: string }) {
  const [confirming, setConfirming] = useState(false);
  const tool = hub.tools.find((t) => t.slug === slug);
  if (!tool) return <div className="empty">Loading…</div>;
  const installed = hub.library.find((l) => l.slug === slug);
  const shortcut = async () => {
    try {
      await native.shortcut(slug);
      hub.toast(`Added ${tool.name} to your desktop.`);
    } catch (e) {
      hub.toast(String(e));
    }
  };
  return (
    <>
      <button className="link back" onClick={() => hub.go({ page: "tools" })}>
        ← All tools
      </button>
      <Hero hub={hub} tool={tool} />
      <ShortcutOffer hub={hub} tool={tool} />
      {tool.screenshots.length > 0 && (
        <div className="row" style={{ marginTop: 16, overflowX: "auto", flexWrap: "nowrap" }}>
          {tool.screenshots.map((s) => (
            <img key={s} src={s} alt="" style={{ height: 180, border: "1px solid var(--line)" }} />
          ))}
        </div>
      )}
      <div className="facts">
        <div className="fact">
          <div className="fact-label">Version</div>
          <div className="fact-value">{installed?.version ?? tool.version}</div>
        </div>
        <div className="fact">
          <div className="fact-label">Works on</div>
          <div className="fact-value">{tool.platforms.map((p) => (p === "mac" ? "Mac" : "Windows")).join(" and ")}</div>
          {testedOn(tool) && <div className="small muted" title="Installed and opened on a clean test computer">Tested on {testedOn(tool)}</div>}
        </div>
        <div className="fact">
          <div className="fact-label">Made by</div>
          <div className="fact-value">
            {tool.credit ? (
              <button className="link author" onClick={() => open(tool.credit!.url)}>
                {tool.credit.name}
              </button>
            ) : (
              <ProfileLink hub={hub} handle={tool.author.handle}>
                {tool.author.name}
              </ProfileLink>
            )}
          </div>
        </div>
        <div className="fact">
          <div className="fact-label">Reviews</div>
          <div className="fact-value">
            <Stars rating={tool.rating} count={tool.reviewCount} />
          </div>
        </div>
      </div>

      <h2>About</h2>
      <div className="prose">{tool.description}</div>

      <h2>Open source</h2>
      <div className="panel">
        <div className="list-row">
          <div className="grow">
            <div className="title">{tool.repoUrl.replace(/^https?:\/\/(www\.)?/, "")}</div>
            <div className="small muted">Anyone can read the code. License: {tool.license}.</div>
          </div>
          <button className="btn" onClick={() => open(tool.repoUrl)}>
            View code
          </button>
        </div>
        <div className="list-row">
          <div className="grow">
            <div className="title">Privacy check</div>
            <div className="small muted">
              {tool.privacy.summary}
              {tool.privacy.checkedAt && ` Checked ${new Date(tool.privacy.checkedAt).toLocaleDateString()}.`}
            </div>
          </div>
          <PrivacyChip privacy={tool.privacy} />
        </div>
      </div>

      {installed && (
        <>
          <h2>Manage</h2>
          <div className="row">
            <button className="btn" onClick={shortcut}>
              {installed.shortcut ? "Re-add desktop shortcut" : "Add desktop shortcut"}
            </button>
            <button className="btn" onClick={() => hub.install(tool)} disabled={!!hub.progress[slug]}>
              Reinstall
            </button>
            {confirming ? (
              <>
                <span className="small muted">Remove {tool.name} from this computer?</span>
                <button className="btn danger" onClick={() => hub.uninstall(tool)}>
                  Yes, uninstall
                </button>
                <button className="btn" onClick={() => setConfirming(false)}>
                  Cancel
                </button>
              </>
            ) : (
              <button className="btn danger" onClick={() => setConfirming(true)}>
                Uninstall
              </button>
            )}
          </div>
          <div className="small muted" style={{ marginTop: 8 }}>
            Toolbay checks for updates every 30 minutes and installs them by itself. If {tool.name} is open, it restarts on the new version.
          </div>
        </>
      )}

      <ToolSpace hub={hub} tool={tool} />

      <Forks hub={hub} tool={tool} />

      <Reviews hub={hub} slug={slug} />

      {tool.inspiredBy && (
        <div className="disclaimer">
          Independent open-source project, not affiliated with or endorsed by {tool.inspiredBy.company}.
        </div>
      )}
    </>
  );
}

// ---------------- Forks ----------------

const isGitHubRepo = (url: string) => /^https:\/\/github\.com\/[^/]+\/[^/]+/.test(url);

function Forks({ hub, tool }: { hub: Hub; tool: Tool }) {
  const [data, setData] = useState<{ count: number; forks: ForkSummary[] }>({ count: 0, forks: [] });
  const [waiting, setWaiting] = useState(false);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api.forks(tool.slug).then((r) => setData(r.data));
  }, [tool.slug]);

  // After GitHub's fork page: check every 4.5 s (up to 10 min) until Toolbay sees the fork.
  useEffect(() => {
    if (!waiting) return;
    const started = Date.now();
    const id = window.setInterval(async () => {
      if (Date.now() - started > 10 * 60 * 1000) {
        setWaiting(false);
        return;
      }
      try {
        const { project } = await api.claimFork(tool.slug);
        if (project) {
          setWaiting(false);
          hub.toast("Your fork is building. You'll find it in My projects.");
          hub.go({ page: "projects" });
        }
      } catch {
        /* keep waiting */
      }
    }, 4500);
    return () => window.clearInterval(id);
  }, [waiting, tool.slug, hub]);

  const mine = !!hub.user?.handle && hub.user.handle === tool.author.handle;
  const canFork = isGitHubRepo(tool.repoUrl) && tool.status !== "coming_soon" && !mine;
  const fork = async () => {
    if (!hub.user) return hub.signIn();
    setBusy(true);
    try {
      const r = await api.fork(tool.slug);
      if (r.mode === "done") {
        hub.toast("Your fork is building. You'll find it in My projects.");
        hub.go({ page: "projects" });
      } else {
        await open(r.mode === "app" ? `https://toolbay.ai${r.url}` : r.url);
        setWaiting(true);
      }
    } catch (e) {
      hub.toast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!canFork && data.count === 0) return null;
  return (
    <>
      <h2>Forks</h2>
      {canFork && (
        <div className="row">
          <button className="btn" disabled={busy || waiting} onClick={fork}>
            Fork{data.count > 0 && <span className="fork-count">{data.count}</span>}
          </button>
          <span className="small muted">
            Make your own copy of {tool.name} to change and share. The credit to the original stays on it.
          </span>
        </div>
      )}
      {waiting && (
        <div className="notice">
          <div className="grow">
            <div className="title">On GitHub, press "Create fork", then come back.</div>
            <div className="small muted">Toolbay will pick it up and start building your copy.</div>
          </div>
          <button className="btn" onClick={() => setWaiting(false)}>
            Cancel
          </button>
        </div>
      )}
      {data.forks.length > 0 && (
        <div className="panel" style={{ marginTop: 12 }}>
          {data.forks.map((f) => (
            <div
              className="list-row clickable"
              key={f.slug}
              onClick={() =>
                hub.tools.some((t) => t.slug === f.slug) ? hub.go({ page: "tools", slug: f.slug }) : open(f.repoUrl)
              }
            >
              <div className="grow">
                <div className="title">{f.name}</div>
                <div className="small muted">
                  by <ProfileLink hub={hub} handle={f.author.handle} /> · {timeAgo(f.createdAt)}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function Reviews({ hub, slug }: { hub: Hub; slug: string }) {
  const [reviews, setReviews] = useState<Review[]>([]);
  const [rating, setRating] = useState(0);
  const [body, setBody] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api.reviews(slug).then((r) => setReviews(r.data));
  }, [slug]);
  const submit = async () => {
    setBusy(true);
    try {
      const { review } = await api.postReview(slug, rating, body.trim());
      setReviews((cur) => [review, ...cur.filter((r) => r.id !== review.id)]);
      setBody("");
      setRating(0);
    } catch {
      hub.toast("Your review didn't post. Check your internet connection and try again.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <h2>Reviews</h2>
      {hub.user ? (
        <div className="composer">
          <div className="star-pick">
            {[1, 2, 3, 4, 5].map((n) => (
              <button key={n} className={n <= rating ? "on" : ""} onClick={() => setRating(n)} aria-label={`${n} stars`}>
                ★
              </button>
            ))}
          </div>
          <textarea placeholder="What did you use it for?" value={body} onChange={(e) => setBody(e.target.value)} />
          <div className="row" style={{ justifyContent: "flex-end" }}>
            <button className="btn primary" disabled={!rating || body.trim().length < 3 || busy} onClick={submit}>
              Post review
            </button>
          </div>
        </div>
      ) : (
        <div className="small muted">
          <button className="link" onClick={hub.signIn}>
            Sign in
          </button>{" "}
          to leave a review.
        </div>
      )}
      {reviews.map((r) => (
        <div className="review" key={r.id}>
          <div className="meta">
            <span className="stars">{"★".repeat(r.rating)}</span>
            <ProfileLink hub={hub} handle={r.author.handle}>
              {r.author.name}
            </ProfileLink>
            <span>{timeAgo(r.createdAt)}</span>
          </div>
          <div>{r.body}</div>
        </div>
      ))}
      {reviews.length === 0 && (
        <div className="small muted" style={{ marginTop: 12 }}>
          No reviews yet.
        </div>
      )}
    </>
  );
}

// ---------------- Running now ----------------

function RunningList({ hub }: { hub: Hub }) {
  return (
    <div className="panel">
      {hub.running.map((r) => {
        const t = hub.tools.find((x) => x.slug === r.slug);
        return (
          <div className="list-row tool-row" key={r.slug}>
            {t ? <Icon tool={t} /> : <div className="icon">{r.name.charAt(0)}</div>}
            <div className="grow">
              <div className="title">{r.name}</div>
              <div className="small muted">{r.window === "hidden" ? "Hidden, still running. Its shortcuts keep working." : "Open on your screen"}</div>
            </div>
            {t ? <ToolControls hub={hub} tool={t} /> : <RunStatus run={r} />}
          </div>
        );
      })}
    </div>
  );
}

export function RunningPage({ hub }: { hub: Hub }) {
  return (
    <>
      <h1>Running now</h1>
      <p className="lede">Tools that are open right now. Hide keeps a tool running in the background.</p>
      {hub.running.length === 0 ? (
        <div className="empty">
          Nothing is running.{" "}
          <button className="link" onClick={() => hub.go({ page: "library" })}>
            Open something from your library
          </button>
        </div>
      ) : (
        <RunningList hub={hub} />
      )}
    </>
  );
}

// ---------------- Library ----------------

/** Tools on the person's other computers that aren't installed here, with one-click Install. */
function OtherComputers({ hub }: { hub: Hub }) {
  const [items, setItems] = useState<{ slug: string; computer: string }[]>([]);
  useEffect(() => {
    const load = () => native.otherComputersTools().then(setItems);
    load();
    const un = native.onSync(load);
    return () => {
      un.then((f) => f());
    };
  }, [hub.library]);
  const tools = items
    .map((i) => ({ ...i, tool: hub.tools.find((t) => t.slug === i.slug) }))
    .filter((i): i is { slug: string; computer: string; tool: Tool } => !!i.tool);
  if (!tools.length) return null;
  return (
    <>
      <h2>On your other computers</h2>
      <div className="panel">
        {tools.map(({ tool, computer }) => (
          <div className="list-row" key={tool.slug}>
            <Icon tool={tool} />
            <div className="grow">
              <div className="title">{tool.name}</div>
              <div className="small muted">Installed on {computer}</div>
            </div>
            <ToolControls hub={hub} tool={tool} />
          </div>
        ))}
      </div>
    </>
  );
}

export function LibraryPage({ hub }: { hub: Hub }) {
  const mine = [...hub.tools, ...hub.projectTools].filter((t) => hub.library.some((l) => l.slug === t.slug));
  return (
    <>
      <div className="page-head">
        <h1>Library</h1>
        <button className="btn" onClick={() => hub.go({ page: "projects" })}>
          Upload project
        </button>
      </div>
      <p className="lede">Everything you've installed. Updates happen by themselves.</p>
      {hub.user && <OtherComputers hub={hub} />}
      {hub.library.length === 0 ? (
        <div className="empty">
          Nothing installed yet.{" "}
          <button className="link" onClick={() => hub.go({ page: "tools" })}>
            Find a tool
          </button>
        </div>
      ) : (
        <div className="panel">
          {hub.library.map((l) => {
            const t = mine.find((x) => x.slug === l.slug);
            return (
              <div className="list-row tool-row clickable" key={l.slug} onClick={() => hub.go({ page: "tools", slug: l.slug })}>
                {t ? <Icon tool={t} /> : <div className="icon">{l.name.charAt(0)}</div>}
                <div className="grow">
                  <div className="title">{t?.name ?? l.name}</div>
                  <div className="small muted">Version {l.version}</div>
                </div>
                <div onClick={(e) => e.stopPropagation()}>
                  {t && <ToolControls hub={hub} tool={t} />}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}

// ---------------- Community ----------------

interface PendingPost {
  tempId: string;
  body: string;
  createdAt: string;
  failed: boolean;
}

export function CommunityPage({ hub }: { hub: Hub }) {
  const [body, setBody] = useState("");
  const [pending, setPending] = useState<PendingPost[]>([]);

  // Optimistic: show it at once, save in the background, keep it with Retry if saving fails.
  const save = async (p: PendingPost) => {
    setPending((cur) => cur.map((x) => (x.tempId === p.tempId ? { ...x, failed: false } : x)));
    try {
      const { post } = await api.createPost(p.body);
      setPending((cur) => cur.filter((x) => x.tempId !== p.tempId));
      hub.addPost(post);
    } catch {
      setPending((cur) => cur.map((x) => (x.tempId === p.tempId ? { ...x, failed: true } : x)));
    }
  };
  const submit = () => {
    const text = body.trim();
    if (!text) return;
    const p = { tempId: `tmp-${Date.now()}`, body: text, createdAt: new Date().toISOString(), failed: false };
    setPending((cur) => [p, ...cur]);
    setBody("");
    save(p);
  };
  const me = hub.user;
  return (
    <>
      <h1>Community</h1>
      <p className="lede">Tips, questions and what people are making with their tools.</p>
      {me ? (
        <div className="composer">
          <textarea
            autoFocus
            placeholder="Share something…"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && e.shiftKey) {
                e.preventDefault();
                submit();
              }
            }}
          />
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="small muted">Shift+Enter to post</span>
            <button className="btn primary" disabled={!body.trim()} onClick={submit}>
              Post
            </button>
          </div>
        </div>
      ) : (
        <div className="small muted">
          <button className="link" onClick={hub.signIn}>
            Sign in
          </button>{" "}
          to post.
        </div>
      )}
      <div style={{ marginTop: 16 }}>
        {me &&
          pending.map((p) => (
            <div className="post" key={p.tempId} style={{ opacity: p.failed ? 1 : 0.7 }}>
              <div className="meta">
                <div className="avatar">{(me.name || me.handle).charAt(0)}</div>
                <span style={{ color: "var(--text)", fontWeight: 600 }}>{me.name || me.handle}</span>
                <span>· {p.failed ? "didn't post" : "posting…"}</span>
                {p.failed && (
                  <button className="link" onClick={() => save(p)}>
                    Retry
                  </button>
                )}
              </div>
              <div className="prose" style={{ fontSize: 14, lineHeight: "21px" }}>
                {p.body}
              </div>
            </div>
          ))}
        {hub.posts.map((p) => (
          <div className="post" key={p.id}>
            <div className="meta">
              {p.author.avatarUrl ? (
                <img className="avatar" src={p.author.avatarUrl} alt="" />
              ) : (
                <div className="avatar">{p.author.name.charAt(0)}</div>
              )}
              <ProfileLink hub={hub} handle={p.author.handle}>
                <span style={{ color: "var(--text)", fontWeight: 600 }}>{p.author.name}</span>
              </ProfileLink>
              <span>· {timeAgo(p.createdAt)}</span>
            </div>
            <div className="prose" style={{ fontSize: 14, lineHeight: "21px" }}>
              {p.body}
            </div>
            {!!p.reactions?.length && (
              <div className="row small muted" style={{ marginTop: 8, gap: 8 }}>
                {p.reactions.map((r) => (
                  <span key={r.kind} className="chip">
                    {r.emoji} {r.count}
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
        {hub.posts.length === 0 && pending.length === 0 && (
          <div className="empty">No posts yet. Be the first to say hello.</div>
        )}
      </div>
    </>
  );
}

// ---------------- Settings ----------------

const APPEARANCES: [Appearance, string][] = [
  ["system", "System"],
  ["light", "Light"],
  ["dark", "Dark"],
];

export function SettingsPage({ hub, setUser }: { hub: Hub; setUser: (u: User | null) => void }) {
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [update, setUpdate] = useState("");
  const [version, setVersion] = useState("");
  useEffect(() => {
    native.settings().then(setSettings);
    import("@tauri-apps/api/app").then((a) => a.getVersion()).then(setVersion).catch(() => {});
  }, []);
  const set = async (key: string, value: string) => {
    await native.setSetting(key, value);
    setSettings((s) => ({ ...s, [key]: value }));
  };
  const checkUpdate = async () => {
    setUpdate("Checking…");
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const u = await check();
      if (!u) return setUpdate("You have the latest version.");
      setUpdate(`Installing Toolbay ${u.version}…`);
      await u.downloadAndInstall();
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch {
      setUpdate("Couldn't check for updates. Check your internet connection and try again.");
    }
  };
  const auto = settings.autoUpdate !== "false";
  const appearance = useAppearance();
  return (
    <>
      <h1>Settings</h1>
      <div className="settings-row">
        <div className="grow">
          <div className="title">Account</div>
          <div className="small muted">
            {hub.user ? `Signed in as ${hub.user.name || hub.user.handle}.` : "Sign in to leave reviews and post. Your browser opens to finish."}
          </div>
        </div>
        {hub.user ? (
          <button
            className="btn"
            onClick={async () => {
              await native.signOut();
              setUser(null);
            }}
          >
            Sign out
          </button>
        ) : (
          <button className="btn primary" onClick={hub.signIn}>
            Sign in
          </button>
        )}
      </div>
      <div className="settings-row">
        <div className="grow">
          <div className="title">Update tools by themselves</div>
          <div className="small muted">New versions install by themselves. An open tool restarts on the new version.</div>
        </div>
        <div className="seg">
          <button className={auto ? "on" : ""} onClick={() => set("autoUpdate", "true")}>
            On
          </button>
          <button className={!auto ? "on" : ""} onClick={() => set("autoUpdate", "false")}>
            Off
          </button>
        </div>
      </div>
      <div className="settings-row">
        <div className="grow">
          <div className="title">Appearance</div>
          <div className="small muted">System follows your computer's light or dark setting.</div>
        </div>
        <div className="seg">
          {APPEARANCES.map(([value, label]) => (
            <button key={value} className={appearance === value ? "on" : ""} onClick={() => setAppearance(value)}>
              {label}
            </button>
          ))}
        </div>
      </div>
      <DropEmailsSetting hub={hub} />
      <div className="settings-row">
        <div className="grow">
          <div className="title">Toolbay</div>
          <div className="small muted">{update || `Version ${version}. Updates install by themselves.`}</div>
        </div>
        <button className="btn" disabled={!isTauri} onClick={checkUpdate}>
          Check for updates
        </button>
      </div>
      <div className="settings-row">
        <div className="grow">
          <div className="title">Source code</div>
          <div className="small muted">Toolbay is free and open source (MIT).</div>
        </div>
        <button className="btn" onClick={() => open("https://github.com/sriptcollector/toolbay-desktop")}>
          View code
        </button>
      </div>
    </>
  );
}
