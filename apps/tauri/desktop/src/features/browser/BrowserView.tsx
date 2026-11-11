/**
 * BrowserView — embedded web browser panel.
 *
 * Phase 2 implementation plan: on-demand webview window (wry-style) spawned
 * only while this panel is open, killed on close — Chromium costs memory only
 * while browsing. Never a hidden background webview.
 */
export function BrowserView() {
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-10 shrink-0 items-center border-b border-sprite-border px-3">
        <span className="text-[12px] uppercase tracking-wide text-sprite-muted">Browser</span>
      </div>
      <div className="flex-1 p-3 text-[13px] text-sprite-muted">
        Browser panel — arriving in Phase 2 (on-demand webview window).
      </div>
    </div>
  );
}
