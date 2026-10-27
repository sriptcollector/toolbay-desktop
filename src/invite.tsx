// Invite friends: your link, how far it got, and the perks (badge, early access, extra votes,
// shout-out). No cash. API: GET /api/v1/me/invite (side-session notes, invites-contract.md).
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Hub } from "./App";
import { callApi, isTauri } from "./lib/api";

const open = (url: string) => (isTauri ? openUrl(url) : window.open(url, "_blank"));

export interface Invite {
  code: string;
  url: string;
  toolUrls: Record<string, string>;
  stats: { visits: number; downloads: number; signups: number; activations: number; pending: number };
  tier: { name: string | null; activations: number; next: { name: string; at: number } | null };
  perks: { earlyAccess: boolean; voteWeight: number };
}

let cached: Promise<Invite> | null = null;
export function loadInvite(refresh = false): Promise<Invite> {
  if (!cached || refresh) {
    cached = callApi<Invite>("GET", "/api/v1/me/invite");
    cached.catch(() => (cached = null));
  }
  return cached;
}

/** Copy text, falling back to a hidden textarea where the clipboard API isn't allowed. */
export async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const t = document.createElement("textarea");
    t.value = text;
    t.style.position = "fixed";
    t.style.opacity = "0";
    document.body.appendChild(t);
    t.select();
    document.execCommand("copy");
    t.remove();
  }
}

/** Copy the link that brings a friend straight to this tool (credited to you when signed in). */
export async function shareTool(hub: Hub, slug: string, name: string) {
  let url = `https://toolbay.ai/tools/${slug}`;
  if (hub.user) {
    try {
      url = (await loadInvite()).toolUrls[slug] ?? url;
    } catch {
      /* fall back to the plain link */
    }
  }
  await copyText(url);
  hub.toast(`Link to ${name} copied. Send it to a friend.`);
}

export function InvitePage({ hub }: { hub: Hub }) {
  const [inv, setInv] = useState<Invite | null | undefined>(undefined);
  useEffect(() => {
    if (!hub.user) return setInv(null);
    loadInvite(true)
      .then(setInv)
      .catch(() => setInv(null));
  }, [hub.user]);

  if (!hub.user) {
    return (
      <>
        <h1>Invite friends</h1>
        <p className="lede">Share Toolbay with friends and get early access to new rebuilds, extra votes and a profile badge.</p>
        <div className="empty">
          <button className="btn primary" onClick={hub.signIn}>
            Sign in to get your link
          </button>
        </div>
      </>
    );
  }
  if (inv === undefined) return <div className="empty">Loading…</div>;
  if (inv === null) {
    return <div className="empty">Couldn't load your invite link. Check your internet connection and try again.</div>;
  }

  const need = inv.tier.next ? inv.tier.next.at - inv.stats.activations : 0;
  const stats: [string, number][] = [
    ["Opened your link", inv.stats.visits],
    ["Downloaded", inv.stats.downloads],
    ["Signed up", inv.stats.signups],
    ["Using a tool", inv.stats.activations],
  ];
  return (
    <>
      <h1>Invite friends</h1>
      <p className="lede">
        Share your link. When a friend signs up and opens a tool, it counts for you. No cash, just good stuff.
      </p>

      <div className="panel invite-link">
        <input className="input grow mono" readOnly value={inv.url} onFocus={(e) => e.currentTarget.select()} />
        <button
          className="btn primary"
          onClick={async () => {
            await copyText(inv.url);
            hub.toast("Invite link copied.");
          }}
        >
          Copy link
        </button>
      </div>

      <div className="facts" style={{ marginTop: 16 }}>
        {stats.map(([label, n]) => (
          <div className="fact" key={label}>
            <div className="fact-label">{label}</div>
            <div className="fact-value" style={{ fontSize: 18 }}>
              {n}
            </div>
          </div>
        ))}
      </div>
      {inv.stats.pending > 0 && (
        <div className="small muted" style={{ marginTop: 8 }}>
          {inv.stats.pending} more being checked. They'll count once they're confirmed.
        </div>
      )}

      <h2>Your level</h2>
      <div className="panel" style={{ padding: "12px 16px" }}>
        <div className="title">{inv.tier.name ?? "Not started yet"}</div>
        <div className="small muted" style={{ marginTop: 4 }}>
          {inv.tier.next
            ? `${need} more ${need === 1 ? "friend" : "friends"} using a tool to reach ${inv.tier.next.name}.`
            : "You're at the top level. Thank you!"}
        </div>
        {inv.tier.next && (
          <div className="progress" style={{ marginTop: 12 }}>
            <div style={{ transform: `scaleX(${Math.min(1, inv.stats.activations / inv.tier.next.at)})` }} />
          </div>
        )}
      </div>

      <h2>What you get</h2>
      <div className="panel">
        {[
          ["A badge on your profile", "Builder at 1 friend, Community builder at 5, Top builder at 25."],
          [
            "Early access to new rebuilds",
            inv.perks.earlyAccess ? "You have it. Look for \"Early access\" on tools." : "Unlocks when your first friend opens a tool.",
          ],
          [
            "Extra votes on the request board",
            inv.perks.voteWeight > 1 ? `Your votes count ${inv.perks.voteWeight}×.` : "Your votes count more as friends join.",
          ],
          ["A shout-out in the weekly drop", "The top inviters each month get a mention."],
        ].map(([title, body]) => (
          <div className="list-row" key={title}>
            <div className="grow">
              <div className="title">{title}</div>
              <div className="small muted">{body}</div>
            </div>
          </div>
        ))}
      </div>
      <div style={{ marginTop: 14 }}>
        <button className="link" onClick={() => open("https://toolbay.ai/invite")}>
          More about invites on toolbay.ai
        </button>
      </div>
    </>
  );
}
