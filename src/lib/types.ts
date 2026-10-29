// Shapes from the Toolbay site API (GET https://toolbay.ai/api/v1/...).
// Keep in step with the contract agreed with the toolbay-site session.

export type Platform = "mac" | "win";

export interface Privacy {
  status: "passed" | "pending" | "failed";
  checkedAt: string | null;
  summary: string;
}

export interface Author {
  name: string;
  handle: string;
}

export interface Tool {
  slug: string;
  name: string;
  tagline: string;
  description: string;
  iconUrl: string;
  screenshots: string[];
  category: string;
  repoUrl: string;
  license: string;
  version: string;
  platforms: Platform[];
  /** Direct download URLs per platform (the website's buttons). The hub installs from `install` instead. */
  downloads?: Partial<Record<Platform, string>>;
  priceCents: number;
  openSource: boolean;
  privacy: Privacy;
  rating: number | null;
  reviewCount: number;
  author: Author;
  status?: "coming_soon" | "beta" | "live" | "early_access";
  /** The product this is an open-source take on. Shown as "Inspired by <name>". */
  inspiredBy?: { name: string; company: string; logoUrl?: string | null } | null;
  /** Results of Toolbay's automatic install/open/close test on clean test computers. */
  compatibility?: {
    platform: string;
    os?: string;
    arch?: string;
    installs: boolean;
    opens: boolean;
    closes: boolean;
    testedAt?: string;
  }[] | null;
  /** For apps by outside maintainers (found by the tool scout): who made it. Shown instead of author. */
  credit?: { name: string; login?: string; url: string; maintainers?: { login: string; url: string }[] } | null;
  /** Set when this tool is a fork of another tool on Toolbay. The credit is permanent. */
  forkedFrom?: { toolSlug: string; builderHandle: string } | null;
  /** Where the hub installs from. Defaults to the GitHub repo in repoUrl. */
  install?: { type: "github"; repo: string } | { type: "manifest"; url: string };
}

export interface Review {
  id: string;
  rating: number;
  body: string;
  createdAt: string;
  author: Author;
}

export interface Post {
  id: string;
  body: string;
  linkUrl?: string | null;
  imageUrls?: string[];
  createdAt: string;
  author: Author & { avatarUrl?: string | null };
  commentCount?: number;
  reactions?: { kind: string; emoji: string; count: number; mine: boolean }[];
}

export interface User {
  id?: string;
  name: string;
  handle: string;
  email?: string;
  avatarUrl?: string;
}

export interface InstalledTool {
  slug: string;
  name: string;
  version: string;
  repo: string;
  kind: string;
  dir: string;
  exe: string;
  launchArgs: string[];
  uninstaller: string | null;
  files: string[];
  shortcut: string | null;
  installedAt: string;
}

/** What installing would fetch (from the tool's toolbay.json on its latest release). */
export interface Resolved {
  manifest: { id: string; name: string; version: string; files: { size?: number }[] };
  entry: { kind: string; size?: number };
  downloadUrl: string;
  releaseTag: string | null;
}

export interface ForkSummary {
  slug: string;
  name: string;
  author: Author;
  repoUrl: string;
  url: string;
  createdAt: string;
}

export interface Profile {
  handle: string;
  /** Invite badge (Builder, Community builder, Top builder), or null. */
  badge?: { name: string; activations: number } | null;
  name: string;
  avatarUrl: string | null;
  bio: string | null;
  /** Builder Social's "good at" line, written from their builds. */
  skills?: string | null;
  location: string | null;
  links: { website: string | null; github: string | null; x: string | null };
  joinedAt: string;
  /** Web path, e.g. "/@orion". */
  url: string;
  pinned: (
    | { type: "tool"; tool: Tool }
    | { type: "repo"; repo: { name: string; url: string; description: string | null } }
    | { type: "post"; post: { id: string; body: string; createdAt: string } }
  )[];
  apps: Tool[];
  forks: Tool[];
  posts: { id: string; body: string; createdAt: string; commentCount: number }[];
  repos: { name: string; url: string; description: string | null; language: string | null; stars: number; updatedAt: string; fork: boolean }[];
}
