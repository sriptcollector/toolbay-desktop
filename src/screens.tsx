import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Hub } from "./App";
import { api, isTauri } from "./lib/api";
import { native, platform, type FlowStatus } from "./lib/native";
import type { Listing, Review, Skill, Tool, User } from "./lib/types";
import { Icon, Price, PrivacyBadge, Stars, Tile, timeAgo } from "./components";

const open = (url: string) => (isTauri ? openUrl(url) : window.open(url, "_blank"));

function matches(item: Listing, q: string) {
  const s = q.trim().toLowerCase();
  if (!s) return true;
  return [item.name, item.tagline, item.category, (item as Tool).replaces ?? ""].some((f) =>
    f.toLowerCase().includes(s),
  );
}

// ---------------- Home ----------------

export function HomePage({ hub }: { hub: Hub }) {
  const greeting = hub.user ? `Welcome back, ${hub.user.name?.split(" ")[0] || hub.user.handle}.` : "Free tools, open source.";
  const installed = hub.library
    .map((l) => hub.tools.find((t) => t.slug === l.slug))
    .filter(Boolean) as Tool[];
  return (
    <>
      <h1 className="display">{greeting}</h1>
      <p className="lede">
        Free, open-source rebuilds of the software you'd otherwise pay for. Every tool shows its source code, license
        and privacy check.
      </p>

      {installed.length > 0 && (
        <>
          <h2>Jump back in</h2>
          <div className="list">
            {installed.slice(0, 4).map((t) => (
              <div className="list-row" key={t.slug}>
                <Icon item={t} />
                <div className="grow">
                  <div className="tile-name">{t.name}</div>
                  <div className="muted small">{t.tagline}</div>
                </div>
                <button className="btn" onClick={() => hub.go({ page: "tools", slug: t.slug })}>
                  Details
                </button>
                {t.kind !== "builtin" && (
                  <button className="btn primary" onClick={() => hub.launch(t.slug)}>
                    Open
                  </button>
                )}
              </div>
            ))}
          </div>
        </>
      )}

      <h2>Tools</h2>
      <div className="grid">
        {hub.tools.slice(0, 6).map((t) => (
          <Tile key={t.slug} item={t} onOpen={() => hub.go({ page: "tools", slug: t.slug })} extra={<Replaces t={t} />} />
        ))}
      </div>

      <h2>Skills for Claude Code</h2>
      <div className="grid">
        {hub.skills.slice(0, 3).map((s) => (
          <Tile key={s.slug} item={s} onOpen={() => hub.go({ page: "skills", slug: s.slug })} />
        ))}
      </div>
    </>
  );
}

function Replaces({ t }: { t: Tool }) {
  return t.replaces ? <span className="badge">Instead of {t.replaces}</span> : null;
}

// ---------------- Tools ----------------

function Browse<T extends Listing>({
  title,
  lede,
  items,
  onOpen,
  extra,
}: {
  title: string;
  lede: string;
  items: T[];
  onOpen: (slug: string) => void;
  extra?: (item: T) => React.ReactNode;
}) {
  const [q, setQ] = useState("");
  const [cat, setCat] = useState("All");
  const cats = useMemo(() => ["All", ...Array.from(new Set(items.map((i) => i.category))).sort()], [items]);
  const shown = items.filter((i) => (cat === "All" || i.category === cat) && matches(i, q));
  return (
    <>
      <h1 className="display">{title}</h1>
      <p className="lede">{lede}</p>
      <div className="row">
        <input className="search" placeholder="Search" value={q} onChange={(e) => setQ(e.target.value)} />
      </div>
      <div className="row" style={{ marginTop: 12 }}>
        {cats.map((c) => (
          <button key={c} className={`chip ${c === cat ? "active" : ""}`} onClick={() => setCat(c)}>
            {c}
          </button>
        ))}
      </div>
      <div className="grid">
        {shown.map((i) => (
          <Tile key={i.slug} item={i} onOpen={() => onOpen(i.slug)} extra={extra?.(i)} />
        ))}
      </div>
      {shown.length === 0 && <div className="empty">Nothing matches that search.</div>}
    </>
  );
}

export function ToolsPage({ hub }: { hub: Hub }) {
  return (
    <Browse
      title="Tools"
      lede="Open-source software you can install with one click. Free, and yours to inspect."
      items={hub.tools}
      onOpen={(slug) => hub.go({ page: "tools", slug })}
      extra={(t) => <Replaces t={t} />}
    />
  );
}

export function SkillsPage({ hub }: { hub: Hub }) {
  return (
    <Browse
      title="Skills"
      lede="Skills for Claude Code and other agents. Install one, then ask your agent to use it."
      items={hub.skills}
      onOpen={(slug) => hub.go({ page: "skills", slug })}
    />
  );
}

// ---------------- Detail ----------------

function Facts({ item }: { item: Listing }) {
  return (
    <div className="facts">
      <div className="fact">
        <div className="fact-label">Version</div>
        <div className="fact-value">{item.version}</div>
      </div>
      <div className="fact">
        <div className="fact-label">License</div>
        <div className="fact-value">{item.license}</div>
      </div>
      <div className="fact">
        <div className="fact-label">Platforms</div>
        <div className="fact-value">{item.platforms.map((p) => (p === "mac" ? "macOS" : "Windows")).join(", ")}</div>
      </div>
      <div className="fact">
        <div className="fact-label">Made by</div>
        <div className="fact-value">{item.author.name}</div>
      </div>
    </div>
  );
}

function Source({ item }: { item: Listing }) {
  return (
    <>
      <h2>Open source</h2>
      <div className="privacy-box">
        <div className="grow" style={{ flex: 1 }}>
          <div style={{ fontWeight: 550 }}>{item.repoUrl.replace(/^https?:\/\/(www\.)?/, "")}</div>
          <div className="muted small">Licensed {item.license}. Read every line before you install.</div>
        </div>
        <button className="btn" onClick={() => open(item.repoUrl)}>
          View source
        </button>
      </div>
      <h2>Privacy check</h2>
      <div className="privacy-box">
        <PrivacyBadge privacy={item.privacy} />
        <div className="small" style={{ flex: 1 }}>
          {item.privacy.summary}
          {item.privacy.checkedAt && (
            <div className="muted">Checked {new Date(item.privacy.checkedAt).toLocaleDateString()}</div>
          )}
        </div>
      </div>
    </>
  );
}

function Screenshots({ item }: { item: Listing }) {
  if (!item.screenshots.length) return null;
  return (
    <div className="shots">
      {item.screenshots.map((s) => (
        <img key={s} src={s} alt="" />
      ))}
    </div>
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
      setReviews((cur) => [review, ...cur]);
      setBody("");
      setRating(0);
    } catch (e) {
      hub.toast(`Couldn't post your review: ${e}`);
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
          <div className="row">
            <div style={{ flex: 1 }} />
            <button className="btn primary" disabled={!rating || !body.trim() || busy} onClick={submit}>
              Post review
            </button>
          </div>
        </div>
      ) : (
        <div className="row muted small">
          <button className="link-btn" onClick={hub.signIn}>
            Sign in
          </button>
          to leave a review.
        </div>
      )}
      {reviews.map((r) => (
        <div className="review" key={r.id}>
          <div className="post-meta">
            <span className="stars">{"★".repeat(r.rating)}</span>
            <span>{r.author.name}</span>
            <span>{timeAgo(r.createdAt)}</span>
          </div>
          <div>{r.body}</div>
        </div>
      ))}
      {reviews.length === 0 && <div className="muted small" style={{ marginTop: 12 }}>No reviews yet.</div>}
    </>
  );
}

function InstallButton({ hub, tool }: { hub: Hub; tool: Tool }) {
  const installed = hub.library.find((l) => l.slug === tool.slug);
  const p = hub.progress[tool.slug];
  const supported = tool.platforms.includes(platform) && !!tool.downloads[platform];
  if (p) {
    const pct = p.total ? p.received / p.total : 0;
    return (
      <div style={{ textAlign: "right" }}>
        <div className="small muted" style={{ marginBottom: 6 }}>
          {p.stage === "downloading"
            ? `Downloading${p.total ? ` ${Math.round(pct * 100)}%` : ` ${(p.received / 1048576).toFixed(0)} MB`}`
            : p.stage === "extracting"
              ? "Unpacking…"
              : "Installing…"}
        </div>
        <div className="progress">
          <div style={{ transform: `scaleX(${p.total ? pct : 0.15})` }} />
        </div>
      </div>
    );
  }
  if (!supported) return <button className="btn" disabled>Not on {platform === "mac" ? "macOS" : "Windows"} yet</button>;
  if (!installed)
    return (
      <button className="btn primary" onClick={() => hub.install(tool)}>
        {tool.priceCents ? `Get · $${(tool.priceCents / 100).toFixed(0)}` : "Install"}
      </button>
    );
  const update = installed.version !== tool.version;
  return (
    <>
      {update && (
        <button className="btn" onClick={() => hub.install(tool)}>
          Update to {tool.version}
        </button>
      )}
      {tool.kind !== "builtin" && (
        <button className="btn primary" onClick={() => hub.launch(tool.slug)}>
          Open
        </button>
      )}
    </>
  );
}

export function ToolDetail({ hub, slug }: { hub: Hub; slug: string }) {
  const tool = hub.tools.find((t) => t.slug === slug);
  if (!tool) return <div className="empty">Loading…</div>;
  const installed = hub.library.find((l) => l.slug === slug);
  return (
    <>
      <button className="link-btn back" onClick={() => hub.go({ page: "tools" })}>
        ← Tools
      </button>
      <div className="detail-head">
        <Icon item={tool} size="lg" />
        <div>
          <h1>{tool.name}</h1>
          <div className="muted">{tool.tagline}</div>
          <div className="row" style={{ marginTop: 8 }}>
            <Price cents={tool.priceCents} />
            <Replaces t={tool} />
            <PrivacyBadge privacy={tool.privacy} />
            <Stars rating={tool.rating} count={tool.reviewCount} />
          </div>
        </div>
        <div className="detail-actions">
          <InstallButton hub={hub} tool={tool} />
        </div>
      </div>
      <Screenshots item={tool} />
      <Facts item={tool} />
      {tool.slug === "toolbay-flow" && <FlowPanel hub={hub} installed={!!installed} />}
      <h2>About</h2>
      <div className="prose">{tool.description}</div>
      <Source item={tool} />
      {installed && tool.kind !== "builtin" && (
        <>
          <h2>Installed</h2>
          <div className="row">
            <span className="muted small" style={{ flex: 1 }}>
              Version {installed.version}, installed {timeAgo(installed.installedAt)}
            </span>
            <button className="btn" onClick={() => native.reveal(slug).catch((e) => hub.toast(String(e)))}>
              Show files
            </button>
            <button className="btn danger" onClick={() => hub.uninstall(slug)}>
              Uninstall
            </button>
          </div>
        </>
      )}
      {installed && tool.kind === "builtin" && (
        <div className="row" style={{ marginTop: 24 }}>
          <div style={{ flex: 1 }} />
          <button className="btn danger" onClick={() => hub.uninstall(slug)}>
            Remove from library
          </button>
        </div>
      )}
      <Reviews hub={hub} slug={slug} />
    </>
  );
}

export function SkillDetail({ hub, slug }: { hub: Hub; slug: string }) {
  const skill = hub.skills.find((s) => s.slug === slug) as Skill | undefined;
  const [busy, setBusy] = useState(false);
  if (!skill) return <div className="empty">Loading…</div>;
  const cmd = skill.install ?? `npx toolbay add ${skill.slug}`;
  const install = async () => {
    setBusy(true);
    try {
      await native.installSkill(skill.slug);
      hub.toast(`${skill.name} installed for Claude Code`);
    } catch (e) {
      hub.toast(String(e).slice(0, 160));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <button className="link-btn back" onClick={() => hub.go({ page: "skills" })}>
        ← Skills
      </button>
      <div className="detail-head">
        <Icon item={skill} size="lg" />
        <div>
          <h1>{skill.name}</h1>
          <div className="muted">{skill.tagline}</div>
          <div className="row" style={{ marginTop: 8 }}>
            <Price cents={skill.priceCents} />
            <PrivacyBadge privacy={skill.privacy} />
            <Stars rating={skill.rating} count={skill.reviewCount} />
          </div>
        </div>
        <div className="detail-actions">
          <button className="btn primary" disabled={busy || !isTauri} onClick={install}>
            {busy ? "Installing…" : "Install"}
          </button>
        </div>
      </div>
      <Facts item={skill} />
      <h2>About</h2>
      <div className="prose">{skill.description}</div>
      <h2>Install from a terminal</h2>
      <code className="cmd">{cmd}</code>
      <Source item={skill} />
    </>
  );
}

// ---------------- Toolbay Flow panel ----------------

function FlowPanel({ hub, installed }: { hub: Hub; installed: boolean }) {
  const [s, setS] = useState<FlowStatus | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!isTauri) return;
    native.flowStatus().then(setS);
    const un = native.onFlow(setS);
    return () => {
      un.then((f) => f());
    };
  }, []);
  if (!installed || !s) return null;
  const p = hub.progress["toolbay-flow"];
  const download = async () => {
    setBusy(true);
    try {
      setS(await native.flowDownloadModel());
    } catch (e) {
      hub.toast(String(e));
    } finally {
      setBusy(false);
    }
  };
  const toggle = async () => {
    try {
      setS(await native.flowEnable(s.state === "off"));
    } catch (e) {
      hub.toast(String(e));
    }
  };
  const label: Record<string, string> = {
    off: "Off",
    idle: "Ready. Hold the keys and talk.",
    recording: "Listening…",
    transcribing: "Transcribing…",
    error: s.message ?? "Something went wrong.",
  };
  return (
    <>
      <h2>Dictation</h2>
      <div className="list">
        <div className="list-row">
          <div className="grow">
            <div style={{ fontWeight: 550 }}>Speech model</div>
            <div className="muted small">
              {s.modelReady ? "whisper base.en, on this computer" : "About 150 MB, downloaded once. Runs offline."}
            </div>
          </div>
          {s.modelReady ? (
            <span className="badge passed">Ready</span>
          ) : p || busy ? (
            <span className="muted small">
              {p?.total ? `${Math.round((p.received / p.total) * 100)}%` : `${((p?.received ?? 0) / 1048576).toFixed(0)} MB`}
            </span>
          ) : (
            <button className="btn primary" onClick={download}>
              Download model
            </button>
          )}
        </div>
        <div className="list-row">
          <div className="grow">
            <div style={{ fontWeight: 550 }}>
              Hold <span className="kbd">{s.hotkey.replace(/\+/g, " + ")}</span> to dictate
            </div>
            <div className="muted small">{label[s.state] ?? s.state}</div>
          </div>
          <button className={`btn ${s.state === "off" ? "primary" : ""}`} disabled={!s.modelReady} onClick={toggle}>
            {s.state === "off" ? "Turn on" : "Turn off"}
          </button>
        </div>
        {s.lastText && (
          <div className="list-row">
            <div className="grow">
              <div className="muted small">Last dictation</div>
              <div>{s.lastText}</div>
            </div>
          </div>
        )}
      </div>
    </>
  );
}

// ---------------- Community ----------------

export function CommunityPage({ hub }: { hub: Hub }) {
  const [body, setBody] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    setBusy(true);
    try {
      await api.createPost(body.trim());
      setBody("");
      await hub.refreshPosts();
    } catch (e) {
      hub.toast(`Couldn't post: ${e}`);
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <h1 className="display">Community</h1>
      <p className="lede">What people are building, fixing and asking about.</p>
      {hub.user ? (
        <div className="composer">
          <textarea placeholder="Share something…" value={body} onChange={(e) => setBody(e.target.value)} />
          <div className="row">
            <div style={{ flex: 1 }} />
            <button className="btn primary" disabled={!body.trim() || busy} onClick={submit}>
              Post
            </button>
          </div>
        </div>
      ) : (
        <div className="row muted small">
          <button className="link-btn" onClick={hub.signIn}>
            Sign in
          </button>
          to post.
        </div>
      )}
      <div style={{ marginTop: 16 }}>
        {hub.posts.map((p) => (
          <div className="post" key={p.id}>
            <div className="post-meta">
              <div className="avatar">{p.author.name.charAt(0)}</div>
              <span style={{ color: "var(--fg)", fontWeight: 550 }}>{p.author.name}</span>
              <span>@{p.author.handle}</span>
              <span>· {timeAgo(p.createdAt)}</span>
              {p.toolSlug && (
                <button className="link-btn" onClick={() => hub.go({ page: "tools", slug: p.toolSlug! })}>
                  · {hub.tools.find((t) => t.slug === p.toolSlug)?.name ?? p.toolSlug}
                </button>
              )}
            </div>
            <div className="prose" style={{ fontSize: 15, lineHeight: "22px" }}>
              {p.body}
            </div>
          </div>
        ))}
        {hub.posts.length === 0 && <div className="empty">No posts yet.</div>}
      </div>
    </>
  );
}

// ---------------- Library ----------------

export function LibraryPage({ hub }: { hub: Hub }) {
  return (
    <>
      <h1 className="display">Library</h1>
      <p className="lede">Everything you've installed. Updates show up here.</p>
      {hub.library.length === 0 ? (
        <div className="empty">
          Nothing installed yet.{" "}
          <button className="link-btn" onClick={() => hub.go({ page: "tools" })}>
            Browse tools
          </button>
        </div>
      ) : (
        <div className="list">
          {hub.library.map((l) => {
            const t = hub.tools.find((x) => x.slug === l.slug);
            const update = t && t.version !== l.version;
            return (
              <div className="list-row clickable" key={l.slug} onClick={() => hub.go({ page: "tools", slug: l.slug })}>
                {t ? <Icon item={t} /> : <div className="icon" />}
                <div className="grow">
                  <div className="tile-name">{l.name}</div>
                  <div className="muted small">
                    {l.version}
                    {update && ` · update to ${t!.version} available`}
                  </div>
                </div>
                <div className="row" onClick={(e) => e.stopPropagation()}>
                  {update && t && (
                    <button className="btn" onClick={() => hub.install(t)}>
                      Update
                    </button>
                  )}
                  {t?.kind !== "builtin" && (
                    <button className="btn primary" onClick={() => hub.launch(l.slug)}>
                      Open
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}

// ---------------- Settings ----------------

export function SettingsPage({ hub, setUser }: { hub: Hub; setUser: (u: User | null) => void }) {
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [update, setUpdate] = useState<string>("");
  useEffect(() => {
    native.settings().then(setSettings);
  }, []);
  const set = async (key: string, value: string) => {
    await native.setSetting(key, value);
    setSettings((s) => ({ ...s, [key]: value }));
  };
  const theme = settings.theme === "dark" ? "dark" : "light";
  const checkUpdate = async () => {
    setUpdate("Checking…");
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const u = await check();
      if (!u) return setUpdate("You're on the latest version.");
      setUpdate(`Downloading ${u.version}…`);
      await u.downloadAndInstall();
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (e) {
      setUpdate(`Couldn't check for updates: ${e}`);
    }
  };
  return (
    <>
      <h1 className="display">Settings</h1>
      <div className="settings-row">
        <div className="grow">
          <div style={{ fontWeight: 550 }}>Account</div>
          <div className="muted small">
            {hub.user ? `Signed in as ${hub.user.name || hub.user.handle}` : "Sign in on toolbay.ai to review and post."}
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
            Sign in with browser
          </button>
        )}
      </div>
      <div className="settings-row">
        <div className="grow">
          <div style={{ fontWeight: 550 }}>Appearance</div>
        </div>
        {(["light", "dark"] as const).map((t) => (
          <button
            key={t}
            className={`chip ${theme === t ? "active" : ""}`}
            onClick={() => {
              document.documentElement.dataset.theme = t;
              set("theme", t);
            }}
          >
            {t === "light" ? "Light" : "Dark"}
          </button>
        ))}
      </div>
      <div className="settings-row">
        <div className="grow">
          <div style={{ fontWeight: 550 }}>Updates</div>
          <div className="muted small">{update || "Toolbay updates itself from signed GitHub releases."}</div>
        </div>
        <button className="btn" disabled={!isTauri} onClick={checkUpdate}>
          Check for updates
        </button>
      </div>
      <div className="settings-row">
        <div className="grow">
          <div style={{ fontWeight: 550 }}>Catalog server</div>
          <div className="muted small">For development. Leave empty for toolbay.ai. Restart to apply.</div>
        </div>
        <input
          className="search"
          style={{ maxWidth: 260 }}
          placeholder="https://toolbay.ai"
          defaultValue={settings.apiBase ?? ""}
          onBlur={(e) => set("apiBase", e.target.value.trim())}
        />
      </div>
      <div className="settings-row">
        <div className="grow">
          <div style={{ fontWeight: 550 }}>About</div>
          <div className="muted small">Toolbay 0.1.0 · MIT · open source</div>
        </div>
        <button className="btn" onClick={() => open("https://github.com/sriptcollector/toolbay-desktop")}>
          Source code
        </button>
      </div>
    </>
  );
}
