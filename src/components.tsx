import { useState } from "react";
import type { Privacy, Tool } from "./lib/types";
import type { Hub } from "./App";

// Each app as a free-standing pixel object (Bedrock's style), shown in tiles and heroes.
const OBJECTS: Record<string, string> = Object.fromEntries(
  Object.entries(import.meta.glob("./assets/objects/*.png", { eager: true, import: "default" }) as Record<string, string>).map(
    ([path, url]) => [path.split("/").pop()!.replace(".png", ""), url],
  ),
);

/** The tool's pixel object, or its icon when it has none yet. */
export function ToolObject({ tool, className = "object", style }: { tool: Pick<Tool, "slug" | "name" | "iconUrl">; className?: string; style?: React.CSSProperties }) {
  const src = OBJECTS[tool.slug];
  if (!src) return <Icon tool={tool} className={`${className} object-fallback`} />;
  return <img className={className} src={src} alt="" draggable={false} style={style} />;
}

export function Icon({ tool, className = "icon" }: { tool: Pick<Tool, "name" | "iconUrl">; className?: string }) {
  const [failed, setFailed] = useState(false);
  return (
    <div className={className}>
      {tool.iconUrl && !failed ? <img src={tool.iconUrl} alt="" onError={() => setFailed(true)} /> : tool.name.charAt(0)}
    </div>
  );
}

export function PrivacyChip({ privacy }: { privacy: Privacy }) {
  const [cls, label] = {
    passed: ["success", "Privacy check passed"],
    pending: ["warn", "Privacy check pending"],
    failed: ["bad", "Privacy check failed"],
  }[privacy.status];
  return (
    <span className={`chip ${cls}`} title={privacy.summary}>
      <span className="dot" />
      {label}
    </span>
  );
}

/** "Inspired by X", with X's small official logo when the catalog provides one (never bundled). */
export function Inspired({ tool }: { tool: Tool }) {
  const [failed, setFailed] = useState(false);
  const ib = tool.inspiredBy;
  if (!ib) return null;
  return (
    <span className="inspired">
      {ib.logoUrl && !failed && <img src={ib.logoUrl} alt="" onError={() => setFailed(true)} />}
      Inspired by {ib.name}
    </span>
  );
}

export function Stars({ rating, count }: { rating: number | null; count: number }) {
  if (!count || rating == null) return <span className="muted small">No reviews yet</span>;
  const full = Math.round(rating);
  return (
    <span className="small">
      <span className="stars">{"★".repeat(full)}{"☆".repeat(5 - full)}</span>{" "}
      <span className="muted">
        {rating.toFixed(1)} · {count}
      </span>
    </span>
  );
}

const paths: Record<string, string> = {
  home: "M3 10.5 12 4l9 6.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z",
  running: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM10 8.5v7l6-3.5z",
  tools: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
  community: "M4 5h16v11H8l-4 4z",
  requests: "M12 4l6 7h-4v9h-4v-9H6z",
  invite: "M15 8a3 3 0 1 0 0-.01M3 20c0-3.3 2.7-6 6-6M9 11a3 3 0 1 0 0-.01M19 13v6M16 16h6",
  library: "M5 4h4v16H5zM10 4h4v16h-4zM15.5 4.5l3.8 1-3.6 14.6-3.8-1z",
  settings:
    "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zm8 3-.1-1.2 2-1.6-2-3.4-2.4.9a8 8 0 0 0-2-1.2L15 2h-4l-.5 2.5a8 8 0 0 0-2 1.2l-2.4-.9-2 3.4 2 1.6L6 12l.1 1.2-2 1.6 2 3.4 2.4-.9a8 8 0 0 0 2 1.2L11 22h4l.5-2.5a8 8 0 0 0 2-1.2l2.4.9 2-3.4-2-1.6z",
};

export function NavIcon({ name }: { name: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round">
      <path d={paths[name]} />
    </svg>
  );
}

export function timeAgo(iso: string) {
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return new Date(iso).toLocaleDateString();
}

/** An author's name, linking to their profile in the hub. */
export function ProfileLink({ hub, handle, children }: { hub: Hub; handle: string; children?: React.ReactNode }) {
  if (!handle) return <>{children}</>;
  return (
    <button
      className="link author"
      onClick={(e) => {
        e.stopPropagation();
        hub.go({ page: "profile", slug: handle });
      }}
    >
      {children ?? `@${handle}`}
    </button>
  );
}

/** "Forked from <tool> by @<builder>", linking to the original tool's page. */
export function ForkedFrom({ hub, from }: { hub: Hub; from: { toolSlug: string; builderHandle: string } }) {
  const original = hub.tools.find((t) => t.slug === from.toolSlug);
  return (
    <span className="inspired">
      Forked from{" "}
      <button
        className="link"
        onClick={(e) => {
          e.stopPropagation();
          hub.go({ page: "tools", slug: from.toolSlug });
        }}
      >
        {original?.name ?? from.toolSlug}
      </button>{" "}
      by <ProfileLink hub={hub} handle={from.builderHandle} />
    </span>
  );
}
