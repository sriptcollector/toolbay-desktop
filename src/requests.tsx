// Rebuild requests: people ask for an open-source rebuild of an app they pay for, and vote.
// API: /api/v1/requests (see side-session notes, hub-requests-feedback-drops-contract.md).
import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Hub } from "./App";
import { callApi, isTauri } from "./lib/api";
import { ProfileLink, timeAgo } from "./components";

const open = (url: string) => (isTauri ? openUrl(url) : window.open(url, "_blank"));

type Status = "OPEN" | "PLANNED" | "BUILDING" | "SHIPPED";
const STATUS: Record<Status, { label: string; cls: string }> = {
  OPEN: { label: "Open", cls: "" },
  PLANNED: { label: "Planned", cls: "warn" },
  BUILDING: { label: "Building", cls: "ok" },
  SHIPPED: { label: "Shipped", cls: "ok" },
};

interface Author {
  name: string;
  handle: string;
  avatarUrl?: string | null;
}
export interface RebuildRequest {
  id: string;
  appName: string;
  appUrl: string | null;
  priceText: string | null;
  mustHaves: string | null;
  status: Status;
  voteCount: number;
  commentCount: number;
  voted: boolean;
  toolSlug: string | null;
  author: Author;
  createdAt: string;
}
interface Comment {
  id: string;
  body: string;
  createdAt: string;
  author: Author;
}

const requestsApi = {
  list: (sort: string) =>
    callApi<{ requests: RebuildRequest[] }>("GET", `/api/v1/requests?sort=${sort}`).then((j) => j.requests),
  get: (id: string) =>
    callApi<{ request?: RebuildRequest; comments?: Comment[]; mergedInto?: string }>("GET", `/api/v1/requests/${id}`),
  create: (body: { appName: string; appUrl?: string; priceText?: string; mustHaves?: string }) =>
    callApi<{ request: RebuildRequest }>("POST", "/api/v1/requests", body).then((j) => j.request),
  vote: (id: string) => callApi<{ voted: boolean; voteCount: number }>("POST", `/api/v1/requests/${id}/vote`),
  comment: (id: string, body: string) =>
    callApi<{ comment: Comment }>("POST", `/api/v1/requests/${id}/comments`, { body }).then((j) => j.comment),
};

function VoteButton({ hub, r, onChange }: { hub: Hub; r: RebuildRequest; onChange: (r: RebuildRequest) => void }) {
  const vote = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!hub.user) return hub.signIn();
    // Optimistic: flip now, settle with the server's numbers.
    onChange({ ...r, voted: !r.voted, voteCount: r.voteCount + (r.voted ? -1 : 1) });
    try {
      const res = await requestsApi.vote(r.id);
      onChange({ ...r, voted: res.voted, voteCount: res.voteCount });
    } catch (err) {
      onChange(r);
      hub.toast(err instanceof Error ? err.message : String(err));
    }
  };
  return (
    <button className={`vote ${r.voted ? "on" : ""}`} onClick={vote} aria-label={r.voted ? "Remove vote" : "Vote"}>
      <span>▲</span>
      {r.voteCount}
    </button>
  );
}

export function RequestsPage({ hub, id }: { hub: Hub; id?: string }) {
  if (id) return <RequestDetail hub={hub} id={id} />;
  return <RequestsBoard hub={hub} />;
}

function RequestsBoard({ hub }: { hub: Hub }) {
  const [sort, setSort] = useState("top");
  const [items, setItems] = useState<RebuildRequest[] | null>(null);
  const [asking, setAsking] = useState(false);

  const load = useCallback(() => {
    requestsApi
      .list(sort)
      .then(setItems)
      .catch(() => setItems((cur) => cur ?? []));
  }, [sort]);
  useEffect(load, [load]);

  const replace = (r: RebuildRequest) => setItems((cur) => cur?.map((x) => (x.id === r.id ? r : x)) ?? null);

  return (
    <>
      <div className="page-head">
        <h1>Requests</h1>
        <button className="btn primary" onClick={() => (hub.user ? setAsking(!asking) : hub.signIn())}>
          Ask for a rebuild
        </button>
      </div>
      <p className="lede">Paying for an app you wish was free? Ask for an open-source rebuild, and vote for the ones you want.</p>
      {asking && (
        <NewRequest
          onDone={(r) => {
            setAsking(false);
            setItems((cur) => [r, ...(cur ?? [])]);
          }}
          onCancel={() => setAsking(false)}
          hub={hub}
        />
      )}
      <div className="seg" style={{ marginBottom: 14 }}>
        {[
          ["top", "Top"],
          ["trending", "Trending"],
          ["new", "New"],
        ].map(([k, label]) => (
          <button key={k} className={sort === k ? "on" : ""} onClick={() => setSort(k)}>
            {label}
          </button>
        ))}
      </div>
      {items === null ? (
        <div className="empty">Loading…</div>
      ) : items.length === 0 ? (
        <div className="empty">No requests yet. Be the first to ask for one.</div>
      ) : (
        <div className="panel">
          {items.map((r) => (
            <div className="list-row clickable request-row" key={r.id} onClick={() => hub.go({ page: "requests", slug: r.id })}>
              <VoteButton hub={hub} r={r} onChange={replace} />
              <div className="grow">
                <div className="title">
                  {r.appName} <span className={`chip ${STATUS[r.status]?.cls ?? ""}`}>{STATUS[r.status]?.label ?? r.status}</span>
                </div>
                {r.mustHaves && <div className="small muted clamp">{r.mustHaves}</div>}
                <div className="small muted">
                  by <ProfileLink hub={hub} handle={r.author.handle}>{r.author.name}</ProfileLink> · {timeAgo(r.createdAt)}
                  {r.commentCount ? ` · ${r.commentCount} comments` : ""}
                  {r.priceText ? ` · costs ${r.priceText}` : ""}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function NewRequest({ hub, onDone, onCancel }: { hub: Hub; onDone: (r: RebuildRequest) => void; onCancel: () => void }) {
  const [appName, setAppName] = useState("");
  const [appUrl, setAppUrl] = useState("");
  const [priceText, setPriceText] = useState("");
  const [mustHaves, setMustHaves] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    if (!appName.trim()) return hub.toast("Which app should be rebuilt? Add its name.");
    setBusy(true);
    try {
      const r = await requestsApi.create({
        appName: appName.trim(),
        appUrl: appUrl.trim() || undefined,
        priceText: priceText.trim() || undefined,
        mustHaves: mustHaves.trim() || undefined,
      });
      onDone(r);
      hub.toast("Request posted. People can vote on it now.");
    } catch (e) {
      hub.toast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="panel upload-box">
      <input className="input" placeholder="App name, e.g. Notion" value={appName} onChange={(e) => setAppName(e.target.value)} autoFocus />
      <div className="upload-row">
        <input className="input grow" placeholder="Its website (optional)" value={appUrl} onChange={(e) => setAppUrl(e.target.value)} />
        <input className="input" placeholder="What it costs (optional)" value={priceText} onChange={(e) => setPriceText(e.target.value)} />
      </div>
      <textarea
        className="input"
        rows={3}
        placeholder="What the rebuild must do (optional)"
        value={mustHaves}
        onChange={(e) => setMustHaves(e.target.value)}
      />
      <div className="row" style={{ justifyContent: "flex-end" }}>
        <button className="btn" onClick={onCancel}>
          Cancel
        </button>
        <button className="btn primary" disabled={busy} onClick={submit}>
          {busy ? "Posting…" : "Post request"}
        </button>
      </div>
    </div>
  );
}

function RequestDetail({ hub, id }: { hub: Hub; id: string }) {
  const [r, setR] = useState<RebuildRequest | null | undefined>(undefined);
  const [comments, setComments] = useState<Comment[]>([]);
  const [body, setBody] = useState("");

  useEffect(() => {
    requestsApi
      .get(id)
      .then((res) => {
        if (res.mergedInto) return hub.go({ page: "requests", slug: res.mergedInto });
        setR(res.request ?? null);
        setComments(res.comments ?? []);
      })
      .catch(() => setR(null));
  }, [id, hub]);

  if (r === undefined) return <div className="empty">Loading…</div>;
  if (r === null) return <div className="empty">That request isn't available anymore.</div>;

  const send = async () => {
    const text = body.trim();
    if (!text) return;
    if (!hub.user) return hub.signIn();
    setBody("");
    const temp: Comment = { id: `tmp-${Date.now()}`, body: text, createdAt: new Date().toISOString(), author: { name: hub.user.name, handle: hub.user.handle } };
    setComments((c) => [...c, temp]);
    try {
      const saved = await requestsApi.comment(r.id, text);
      setComments((c) => c.map((x) => (x.id === temp.id ? saved : x)));
      setR({ ...r, commentCount: r.commentCount + 1 });
    } catch (e) {
      setComments((c) => c.filter((x) => x.id !== temp.id));
      setBody(text);
      hub.toast(e instanceof Error ? e.message : String(e));
    }
  };

  const tool = r.toolSlug ? hub.tools.find((t) => t.slug === r.toolSlug) : undefined;
  return (
    <>
      <button className="link back" onClick={() => hub.go({ page: "requests" })}>
        ← All requests
      </button>
      <div className="request-head">
        <VoteButton hub={hub} r={r} onChange={setR} />
        <div className="grow">
          <h1>{r.appName}</h1>
          <div className="row" style={{ gap: 8 }}>
            <span className={`chip ${STATUS[r.status]?.cls ?? ""}`}>{STATUS[r.status]?.label ?? r.status}</span>
            <span className="small muted">
              by <ProfileLink hub={hub} handle={r.author.handle}>{r.author.name}</ProfileLink> · {timeAgo(r.createdAt)}
              {r.priceText ? ` · costs ${r.priceText}` : ""}
            </span>
          </div>
        </div>
      </div>
      {r.mustHaves && (
        <>
          <h2>Must-haves</h2>
          <div className="prose">{r.mustHaves}</div>
        </>
      )}
      <div className="row" style={{ marginTop: 18 }}>
        {r.appUrl && (
          <button className="btn" onClick={() => open(r.appUrl!)}>
            Visit {r.appName}
          </button>
        )}
        {tool ? (
          <button className="btn primary" onClick={() => hub.go({ page: "tools", slug: tool.slug })}>
            Get {tool.name}
          </button>
        ) : (
          <button className="btn" onClick={() => open(`https://toolbay.ai/projects?request=${encodeURIComponent(r.id)}`)}>
            I built this
          </button>
        )}
      </div>

      <h2>Comments</h2>
      {comments.map((c) => (
        <div className="post" key={c.id}>
          <div className="meta">
            <ProfileLink hub={hub} handle={c.author.handle}>
              <span style={{ color: "var(--text)", fontWeight: 600 }}>{c.author.name}</span>
            </ProfileLink>
            <span>· {c.id.startsWith("tmp-") ? "posting…" : timeAgo(c.createdAt)}</span>
          </div>
          <div className="prose" style={{ fontSize: 15, lineHeight: "22px" }}>
            {c.body}
          </div>
        </div>
      ))}
      {comments.length === 0 && <div className="small muted">No comments yet.</div>}
      {hub.user ? (
        <div className="composer" style={{ marginTop: 14 }}>
          <textarea
            placeholder="Add a comment…"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
          />
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="small muted">Shift+Enter to send</span>
            <button className="btn primary" disabled={!body.trim()} onClick={send}>
              Comment
            </button>
          </div>
        </div>
      ) : (
        <div className="small muted" style={{ marginTop: 12 }}>
          <button className="link" onClick={hub.signIn}>
            Sign in
          </button>{" "}
          to comment.
        </div>
      )}
    </>
  );
}
