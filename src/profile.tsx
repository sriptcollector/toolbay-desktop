// A person's profile (GET /api/v1/users/:handle): their apps, forks, posts and public GitHub repos.
// No follower counts or endorsements, by design. Reached from any author name in the hub.
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Hub } from "./App";
import { api, isTauri } from "./lib/api";
import type { Profile } from "./lib/types";
import { Icon, timeAgo } from "./components";

const open = (url: string) => (isTauri ? openUrl(url) : window.open(url, "_blank"));
const webUrl = (p: Profile) => `https://toolbay.ai${p.url || `/@${p.handle}`}`;

export function ProfilePage({ hub, handle }: { hub: Hub; handle: string }) {
  const [p, setP] = useState<Profile | null | undefined>(undefined);
  useEffect(() => {
    api.user(handle).then(setP);
  }, [handle]);

  if (p === undefined) return <div className="empty">Loading…</div>;
  if (p === null) {
    return (
      <div className="empty">
        Couldn't find @{handle}.{" "}
        <button className="link" onClick={() => hub.go({ page: "home" })}>
          Go home
        </button>
      </div>
    );
  }

  const links = [
    p.links.website && { label: "Website", url: p.links.website },
    p.links.github && { label: "GitHub", url: p.links.github },
    p.links.x && { label: "X", url: p.links.x },
  ].filter(Boolean) as { label: string; url: string }[];
  const isMe = hub.user?.handle === p.handle;

  return (
    <>
      <div className="profile-head">
        {p.avatarUrl ? (
          <img className="profile-avatar" src={p.avatarUrl} alt="" />
        ) : (
          <div className="profile-avatar">{(p.name || p.handle).charAt(0).toUpperCase()}</div>
        )}
        <div className="grow">
          <h1>
            {p.name || `@${p.handle}`}
            {p.badge && (
              <span className="chip ok" style={{ marginLeft: 10, verticalAlign: "middle" }} title={`${p.badge.activations} friends using Toolbay`}>
                {p.badge.name}
              </span>
            )}
          </h1>
          <div className="muted">
            @{p.handle}
            {p.location ? ` · ${p.location}` : ""} · joined {new Date(p.joinedAt).toLocaleDateString()}
          </div>
          {p.skills && <div className="small" style={{ marginTop: 6, color: "var(--text)" }}>Good at: {p.skills}</div>}
          {p.bio && <div className="prose" style={{ marginTop: 8, fontSize: 14 }}>{p.bio}</div>}
          <div className="row" style={{ marginTop: 12, gap: 8 }}>
            {links.map((l) => (
              <button key={l.label} className="btn small" onClick={() => open(l.url)}>
                {l.label}
              </button>
            ))}
            <button className="btn small" onClick={() => open(webUrl(p))}>
              View on toolbay.ai
            </button>
            {isMe && (
              <button className="btn small" onClick={() => hub.go({ page: "projects" })}>
                Upload your app
              </button>
            )}
          </div>
        </div>
      </div>

      {p.pinned.length > 0 && (
        <>
          <h2>Pinned</h2>
          <div className="panel">
            {p.pinned.map((pin, i) => (
              <PinnedRow key={i} hub={hub} pin={pin} />
            ))}
          </div>
        </>
      )}

      {p.apps.length > 0 && (
        <>
          <h2>Apps</h2>
          <ToolList hub={hub} tools={p.apps} />
        </>
      )}

      {p.forks.length > 0 && (
        <>
          <h2>Forks</h2>
          <ToolList hub={hub} tools={p.forks} />
        </>
      )}

      {p.posts.length > 0 && (
        <>
          <h2>Posts</h2>
          {p.posts.map((post) => (
            <div className="post" key={post.id}>
              <div className="meta">{timeAgo(post.createdAt)}{post.commentCount ? ` · ${post.commentCount} comments` : ""}</div>
              <div className="prose" style={{ fontSize: 14, lineHeight: "21px" }}>
                {post.body}
              </div>
            </div>
          ))}
        </>
      )}

      {p.repos.length > 0 && (
        <>
          <h2>Open-source repos</h2>
          <div className="panel">
            {p.repos.map((r) => (
              <div className="list-row clickable" key={r.url} onClick={() => open(r.url)}>
                <div className="grow">
                  <div className="title">
                    {r.name}
                    {r.fork && <span className="chip" style={{ marginLeft: 8 }}>fork</span>}
                  </div>
                  {r.description && <div className="small muted">{r.description}</div>}
                </div>
                <div className="small muted mono" style={{ textAlign: "right" }}>
                  {[r.language, r.stars ? `★ ${r.stars}` : null].filter(Boolean).join(" · ")}
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      {p.apps.length + p.forks.length + p.posts.length + p.repos.length + p.pinned.length === 0 && (
        <div className="empty">Nothing here yet.</div>
      )}
    </>
  );
}

function ToolList({ hub, tools }: { hub: Hub; tools: Profile["apps"] }) {
  return (
    <div className="panel">
      {tools.map((t) => (
        <div className="list-row clickable" key={t.slug} onClick={() => hub.go({ page: "tools", slug: t.slug })}>
          <Icon tool={t} />
          <div className="grow">
            <div className="title">{t.name}</div>
            <div className="small muted">{t.tagline}</div>
          </div>
        </div>
      ))}
    </div>
  );
}

function PinnedRow({ hub, pin }: { hub: Hub; pin: Profile["pinned"][number] }) {
  if (pin.type === "tool") {
    return (
      <div className="list-row clickable" onClick={() => hub.go({ page: "tools", slug: pin.tool.slug })}>
        <Icon tool={pin.tool} />
        <div className="grow">
          <div className="title">{pin.tool.name}</div>
          <div className="small muted">{pin.tool.tagline}</div>
        </div>
      </div>
    );
  }
  if (pin.type === "repo") {
    return (
      <div className="list-row clickable" onClick={() => open(pin.repo.url)}>
        <div className="grow">
          <div className="title">{pin.repo.name}</div>
          {pin.repo.description && <div className="small muted">{pin.repo.description}</div>}
        </div>
      </div>
    );
  }
  return (
    <div className="list-row clickable" onClick={() => hub.go({ page: "community" })}>
      <div className="grow">
        <div className="small muted">Post · {timeAgo(pin.post.createdAt)}</div>
        <div>{pin.post.body}</div>
      </div>
    </div>
  );
}
