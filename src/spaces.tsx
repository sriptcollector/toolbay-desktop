// Per-tool community space (feedback, bugs, ideas, show-off) and the weekly drop.
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Hub } from "./App";
import { callApi, isTauri } from "./lib/api";
import type { Tool } from "./lib/types";
import { ProfileLink, timeAgo } from "./components";

const open = (url: string) => (isTauri ? openUrl(url) : window.open(url, "_blank"));

// ---------------- Tool space ----------------

type Tag = "feedback" | "bug" | "idea" | "showoff";
const TAGS: { tag: Tag; label: string }[] = [
  { tag: "feedback", label: "Feedback" },
  { tag: "bug", label: "Bugs" },
  { tag: "idea", label: "Ideas" },
  { tag: "showoff", label: "Show off" },
];
const TAG_LABEL: Record<Tag, string> = { feedback: "Feedback", bug: "Bug", idea: "Idea", showoff: "Show off" };

interface SpacePost {
  id: string;
  body: string;
  createdAt: string;
  author: { name: string; handle: string; avatarUrl?: string | null };
  tag?: Tag | null;
  status?: "open" | "planned" | "fixed" | null;
  fixedInVersion?: string | null;
  visibility?: "public" | "maker";
  isMaker?: boolean;
  isTeam?: boolean;
  commentCount?: number;
}

export function ToolSpace({ hub, tool }: { hub: Hub; tool: Tool }) {
  const [tag, setTag] = useState<Tag | null>(null);
  const [posts, setPosts] = useState<SpacePost[] | null>(null);
  const [type, setType] = useState<Tag>("feedback");
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const q = `tool=${encodeURIComponent(tool.slug)}${tag ? `&tag=${tag}` : ""}`;
    callApi<{ posts: SpacePost[] }>("GET", `/api/v1/community/posts?${q}`)
      .then((j) => setPosts(j.posts))
      .catch(() => setPosts([]));
  }, [tool.slug, tag]);

  const send = async () => {
    const body = text.trim();
    if (body.length < 3) return hub.toast("Write a few more words first.");
    if (!hub.user) return hub.signIn();
    setBusy(true);
    try {
      const { post } = await callApi<{ post: SpacePost }>("POST", `/api/v1/tools/${tool.slug}/feedback`, {
        type,
        text: body,
        os: navigator.userAgent.includes("Mac") ? "macOS" : "Windows",
      });
      setText("");
      setPosts((cur) => [post, ...(cur ?? [])]);
      hub.toast(post.visibility === "maker" ? `Sent privately to the maker of ${tool.name}.` : "Posted. Thanks!");
    } catch (e) {
      hub.toast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h2>Community</h2>
      <div className="row" style={{ gap: 8, marginBottom: 12 }}>
        <button className={`chip-btn ${tag === null ? "on" : ""}`} onClick={() => setTag(null)}>
          All
        </button>
        {TAGS.map((t) => (
          <button key={t.tag} className={`chip-btn ${tag === t.tag ? "on" : ""}`} onClick={() => setTag(t.tag)}>
            {t.label}
          </button>
        ))}
      </div>
      {hub.user ? (
        <div className="composer">
          <div className="row" style={{ gap: 6 }}>
            {TAGS.map((t) => (
              <button key={t.tag} className={`chip-btn small ${type === t.tag ? "on" : ""}`} onClick={() => setType(t.tag)}>
                {TAG_LABEL[t.tag]}
              </button>
            ))}
          </div>
          <textarea
            placeholder={
              type === "bug"
                ? `What went wrong in ${tool.name}?`
                : type === "idea"
                  ? `What should ${tool.name} do next?`
                  : type === "showoff"
                    ? `Show what you made with ${tool.name}`
                    : `What do you think of ${tool.name}?`
            }
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
          />
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="small muted">Shift+Enter to post</span>
            <button className="btn primary" disabled={busy || text.trim().length < 3} onClick={send}>
              {busy ? "Posting…" : "Post"}
            </button>
          </div>
        </div>
      ) : (
        <div className="small muted">
          <button className="link" onClick={hub.signIn}>
            Sign in
          </button>{" "}
          to share feedback, report a bug or suggest an idea.
        </div>
      )}
      <div style={{ marginTop: 8 }}>
        {posts === null && <div className="small muted">Loading…</div>}
        {posts?.length === 0 && <div className="small muted" style={{ marginTop: 12 }}>Nothing here yet.</div>}
        {posts?.map((p) => (
          <div className="post" key={p.id}>
            <div className="meta">
              <ProfileLink hub={hub} handle={p.author.handle}>
                <span style={{ color: "var(--text)", fontWeight: 600 }}>{p.author.name}</span>
              </ProfileLink>
              {p.isMaker && <span className="chip ok">Maker</span>}
              {p.isTeam && <span className="chip">Toolbay</span>}
              {p.tag && <span className="chip">{TAG_LABEL[p.tag]}</span>}
              {p.status && (
                <span className={`chip ${p.status === "fixed" ? "ok" : p.status === "planned" ? "warn" : ""}`}>
                  {p.status === "fixed"
                    ? p.fixedInVersion
                      ? `Fixed in v${p.fixedInVersion.replace(/^v/, "")}`
                      : "Fixed"
                    : p.status === "planned"
                      ? "Planned"
                      : "Open"}
                </span>
              )}
              {p.visibility === "maker" && <span className="chip">Only the maker sees this</span>}
              <span>· {timeAgo(p.createdAt)}</span>
            </div>
            <div className="prose" style={{ fontSize: 14, lineHeight: "21px" }}>
              {p.body}
            </div>
          </div>
        ))}
      </div>
    </>
  );
}

// ---------------- Weekly drop ----------------

interface Drop {
  weekOf: string;
  title: string;
  intro: string;
  body: string;
  publishedAt: string;
  url: string;
}

/** Light markdown: "## " headings, "- " bullets, [text](link), blank-line paragraphs. No HTML. */
function Markdown({ text }: { text: string }) {
  const inline = (line: string, key: string) => {
    const parts: React.ReactNode[] = [];
    const re = /\[([^\]]+)\]\(([^)\s]+)\)/g;
    let last = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(line))) {
      parts.push(line.slice(last, m.index));
      const href = m[2].startsWith("/") ? `https://toolbay.ai${m[2]}` : m[2];
      parts.push(
        <button key={`${key}-${m.index}`} className="link inline" onClick={() => href.startsWith("http") && open(href)}>
          {m[1]}
        </button>,
      );
      last = m.index + m[0].length;
    }
    parts.push(line.slice(last));
    return parts;
  };
  const blocks: React.ReactNode[] = [];
  let bullets: string[] = [];
  const flush = () => {
    if (bullets.length) {
      const items = bullets;
      blocks.push(
        <ul key={`ul-${blocks.length}`} className="md-list">
          {items.map((b, i) => (
            <li key={i}>{inline(b, `li-${blocks.length}-${i}`)}</li>
          ))}
        </ul>,
      );
      bullets = [];
    }
  };
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.trimEnd();
    if (line.startsWith("- ")) return bullets.push(line.slice(2));
    flush();
    if (!line.trim()) return;
    if (line.startsWith("## ") || line.startsWith("### ")) {
      blocks.push(
        <div key={i} className="md-h">
          {line.replace(/^#+\s*/, "")}
        </div>,
      );
    } else {
      blocks.push(<p key={i}>{inline(line, `p-${i}`)}</p>);
    }
  });
  flush();
  return <div className="md">{blocks}</div>;
}

export function DropCard() {
  const [drop, setDrop] = useState<Drop | null>(null);
  const [expanded, setExpanded] = useState(false);
  useEffect(() => {
    callApi<{ drop: Drop | null }>("GET", "/api/v1/drops/latest")
      .then((j) => setDrop(j.drop))
      .catch(() => setDrop(null));
  }, []);
  if (!drop) return null;
  return (
    <>
      <h2>This week's drop</h2>
      <div className="panel drop-card">
        <div className="small muted mono">Week of {new Date(drop.weekOf).toLocaleDateString()}</div>
        <div className="title" style={{ fontSize: 15, marginTop: 4 }}>
          {drop.title}
        </div>
        <div className="prose" style={{ fontSize: 14, marginTop: 4 }}>
          {drop.intro}
        </div>
        {expanded && <Markdown text={drop.body} />}
        <div className="row" style={{ marginTop: 12 }}>
          <button className="btn" onClick={() => setExpanded(!expanded)}>
            {expanded ? "Show less" : "Read it"}
          </button>
          <button className="link" onClick={() => open(drop.url.startsWith("/") ? `https://toolbay.ai${drop.url}` : drop.url)}>
            Open on toolbay.ai
          </button>
        </div>
      </div>
    </>
  );
}

/** Settings row: "Email me the weekly drop". */
export function DropEmailsSetting({ hub }: { hub: Hub }) {
  const [on, setOn] = useState<boolean | null>(null);
  useEffect(() => {
    if (!hub.user) return;
    callApi<{ on: boolean }>("GET", "/api/v1/me/drop-emails")
      .then((j) => setOn(j.on))
      .catch(() => setOn(null));
  }, [hub.user]);
  if (!hub.user) return null;
  const set = async (v: boolean) => {
    const prev = on;
    setOn(v);
    try {
      setOn((await callApi<{ on: boolean }>("POST", "/api/v1/me/drop-emails", { on: v })).on);
    } catch (e) {
      setOn(prev);
      hub.toast(e instanceof Error ? e.message : String(e));
    }
  };
  return (
    <div className="settings-row">
      <div className="grow">
        <div className="title">Email me the weekly drop</div>
        <div className="small muted">A short email each week with new tools and updates.</div>
      </div>
      <div className="seg">
        <button className={on === true ? "on" : ""} disabled={on === null} onClick={() => set(true)}>
          On
        </button>
        <button className={on === false ? "on" : ""} disabled={on === null} onClick={() => set(false)}>
          Off
        </button>
      </div>
    </div>
  );
}
