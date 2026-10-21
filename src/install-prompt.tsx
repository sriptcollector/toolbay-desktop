// "Install <tool>?" in one step: shown on first launch after downloading Toolbay from a tool's page
// (the installer's file name says which tool, see src-tauri/src/firstrun.rs) and for
// toolbay://install/<slug>. No account needed; signing in is only for sync.
import { useEffect } from "react";
import type { Hub } from "./App";
import { Icon, Inspired } from "./components";

export interface Wish {
  slug: string;
  firstRun?: boolean;
}

export function InstallPrompt({ hub, wish, onDone }: { hub: Hub; wish: Wish; onDone: () => void }) {
  const tool = [...hub.tools, ...hub.projectTools].find((t) => t.slug === wish.slug);
  const installed = hub.library.some((t) => t.slug === wish.slug);

  // Unknown tool once the catalog is in: say so instead of waiting forever.
  useEffect(() => {
    if (!tool && hub.loaded) {
      hub.toast("Couldn't find that tool. It may have been removed. Have a look around instead.");
      onDone();
    }
  }, [tool, hub.loaded]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!tool) return null;

  const go = async () => {
    onDone();
    hub.go({ page: "tools", slug: tool.slug });
    if (installed || (await hub.install(tool))) hub.open(tool);
  };

  return (
    <div className="prompt-backdrop" role="dialog" aria-modal="true" aria-label={`Install ${tool.name}`}>
      <div className="prompt">
        <Icon tool={tool} className="hero-icon" />
        {wish.firstRun && <div className="prompt-kicker">Welcome to Toolbay</div>}
        <h1>{installed ? `Open ${tool.name}?` : `Install ${tool.name}?`}</h1>
        <p className="lede">{tool.tagline}</p>
        <Inspired tool={tool} />
        <button className="btn primary big" onClick={go} autoFocus>
          {installed ? "Open" : "Install"}
        </button>
        <div className="small muted">Free and open source. No account needed.</div>
        <button className="link small" onClick={onDone}>
          Not now
        </button>
      </div>
    </div>
  );
}
