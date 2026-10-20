import { useState } from "react";
import type { Privacy, Tool } from "./lib/types";

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
    passed: ["ok", "Privacy check passed"],
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
