// Upload an app package: step 1, give the AI the packaging prompt; step 2, drag the zip in.
// The zip goes to POST /api/v1/packages through Rust (package_upload), which adds the token.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Hub } from "./App";
import { callApi, isTauri } from "./lib/api";
import { copyText } from "./invite";
import type { Project } from "./lib/projects";

type Result =
  | { kind: "ok"; project: Project; warnings: string[] }
  | { kind: "fix"; error: string; errors: string[] }
  | { kind: "error"; error: string };

export function PackageDrop({ hub, onUploaded }: { hub: Hub; onUploaded: () => void }) {
  const [hovering, setHovering] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Result | null>(null);

  const upload = async (path: string) => {
    setBusy(true);
    setResult(null);
    try {
      const res = await invoke<{ status: number; body: string }>("package_upload", { path });
      let json: any = {};
      try {
        json = JSON.parse(res.body);
      } catch {
        /* keep {} */
      }
      if (res.status === 201 || res.status === 200) {
        setResult({ kind: "ok", project: json.project, warnings: json.warnings ?? [] });
        onUploaded();
      } else if (res.status === 422) {
        setResult({ kind: "fix", error: json.error ?? "The zip needs a few fixes.", errors: json.errors ?? [] });
      } else {
        setResult({ kind: "error", error: json.error ?? "That upload didn't work. Try again in a minute." });
      }
    } catch (e) {
      setResult({ kind: "error", error: String(e) });
    } finally {
      setBusy(false);
    }
  };

  // Files dropped anywhere on the window while this screen is open.
  useEffect(() => {
    if (!isTauri) return;
    const un = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "over" || p.type === "enter") setHovering(true);
      else if (p.type === "leave") setHovering(false);
      else if (p.type === "drop") {
        setHovering(false);
        const zip = p.paths.find((x) => x.toLowerCase().endsWith(".zip")) ?? p.paths[0];
        if (zip) upload(zip);
      }
    });
    return () => {
      un.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const copyPrompt = async () => {
    try {
      const { prompt } = await callApi<{ prompt: string }>("GET", "/api/v1/packages/prompt");
      await copyText(prompt);
      hub.toast("Prompt copied. Paste it into your AI (Claude, ChatGPT, Cursor…).");
    } catch {
      openUrl("https://toolbay.ai/docs/package");
    }
  };

  const choose = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ multiple: false, filters: [{ name: "App package", extensions: ["zip"] }] });
    if (typeof picked === "string") upload(picked);
  };

  return (
    <div className="panel upload-box">
      <div className="title">Or upload an app your AI made</div>
      <div className="steps">
        <div className="step">
          <span className="step-n">1</span>
          <div className="grow">
            <div>Give this prompt to your AI. It packages your app the way Toolbay needs.</div>
            <button className="btn small" style={{ marginTop: 8 }} onClick={copyPrompt}>
              Copy the prompt
            </button>
          </div>
        </div>
        <div className="step">
          <span className="step-n">2</span>
          <div className="grow">
            <div
              className={`drop-zone ${hovering ? "over" : ""}`}
              onClick={() => !busy && choose()}
              role="button"
              tabIndex={0}
            >
              {busy ? "Uploading and checking…" : hovering ? "Drop it" : "Drag the zip in here, or click to choose it"}
            </div>
          </div>
        </div>
      </div>
      {result?.kind === "ok" && (
        <div className="small">
          <span className="ok">Uploaded.</span>{" "}
          {result.project?.autoPublished
            ? "It passed every check and will be listed once it's built."
            : (result.project as any)?.holdReasons?.length
              ? "It's being built. Before it's listed, Toolbay will look at: " + (result.project as any).holdReasons.join("; ")
              : "It's being built. You'll see it below."}
          {result.warnings.length > 0 && (
            <ul className="privacy-list">
              {result.warnings.map((w) => (
                <li key={w} className="warn">
                  ! {w}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      {result?.kind === "fix" && (
        <div className="small">
          <div className="danger">{result.error}</div>
          <ul className="privacy-list">
            {result.errors.map((e) => (
              <li key={e}>✕ {e}</li>
            ))}
          </ul>
          <div className="muted" style={{ marginTop: 6 }}>
            Ask your AI to fix these and make the zip again.
          </div>
        </div>
      )}
      {result?.kind === "error" && <div className="small danger">{result.error}</div>}
    </div>
  );
}
