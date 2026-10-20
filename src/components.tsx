import { useState } from "react";
import type { Listing, Privacy } from "./lib/types";

const PALETTE = ["#6c358c", "#034f46", "#7f1c34", "#30302f", "#a26ec1", "#b5562d", "#2d5fb5"];

export function Icon({ item, size }: { item: Pick<Listing, "name" | "iconUrl" | "slug">; size?: "lg" }) {
  const [failed, setFailed] = useState(false);
  const color = PALETTE[[...item.slug].reduce((a, c) => a + c.charCodeAt(0), 0) % PALETTE.length];
  return (
    <div className={`icon ${size ?? ""}`} style={{ background: color }}>
      {item.iconUrl && !failed ? (
        <img src={item.iconUrl} alt="" onError={() => setFailed(true)} />
      ) : (
        item.name.charAt(0)
      )}
    </div>
  );
}

export function PrivacyBadge({ privacy }: { privacy: Privacy }) {
  const label = { passed: "Privacy check passed", pending: "Privacy check pending", failed: "Privacy check failed" }[
    privacy.status
  ];
  return (
    <span className={`badge ${privacy.status}`} title={privacy.summary}>
      <span className="dot" />
      {label}
    </span>
  );
}

export function Stars({ rating, count }: { rating: number; count: number }) {
  if (!count) return <span className="muted small">No reviews yet</span>;
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

export function Price({ cents }: { cents: number }) {
  return <span className="badge">{cents ? `$${(cents / 100).toFixed(cents % 100 ? 2 : 0)}` : "Free"}</span>;
}

export function Tile({ item, onOpen, extra }: { item: Listing; onOpen: () => void; extra?: React.ReactNode }) {
  return (
    <button className="tile" onClick={onOpen}>
      <div className="tile-head">
        <Icon item={item} />
        <div style={{ minWidth: 0 }}>
          <div className="tile-name">{item.name}</div>
          <div className="muted small">{item.category}</div>
        </div>
      </div>
      <div className="tile-tagline">{item.tagline}</div>
      <div className="tile-foot">
        <Price cents={item.priceCents} />
        <span className="badge">{item.license}</span>
        <PrivacyBadge privacy={item.privacy} />
        {extra}
      </div>
    </button>
  );
}

const paths: Record<string, string> = {
  home: "M3 10.5 12 4l9 6.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z",
  tools: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
  skills: "M12 3l2.6 5.6L20 9.5l-4 4 1 6-5-2.8L7 19.5l1-6-4-4 5.4-.9z",
  community: "M4 5h16v11H8l-4 4z",
  library: "M5 4h4v16H5zM10 4h4v16h-4zM15.5 4.5l3.8 1 -3.6 14.6-3.8-1z",
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
