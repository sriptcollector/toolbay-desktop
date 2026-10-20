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

export interface Listing {
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
  priceCents: number;
  openSource: boolean;
  privacy: Privacy;
  rating: number;
  reviewCount: number;
  author: Author;
}

export interface Tool extends Listing {
  /** Direct download URL per platform. "builtin:<slug>" means the tool ships inside this app. */
  downloads: Partial<Record<Platform, string>>;
  /** Optional, not in the base contract: the paid product this replaces. */
  replaces?: string;
  /** Optional: program to start inside the unpacked download, with args, per platform. */
  launch?: Partial<Record<Platform, string>>;
  /** Optional: "app" | "cli" | "folder" | "builtin". */
  kind?: string;
}

export interface Skill extends Listing {
  /** Optional: the command that installs it. */
  install?: string;
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
  createdAt: string;
  author: Author;
  toolSlug: string | null;
  replyCount: number;
  likeCount: number;
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
  path: string;
  launchPath: string | null;
  installedAt: string;
}
