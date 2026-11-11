/**
 * TerminalView — embedded terminal panel.
 *
 * Phase 2 implementation plan: xterm.js frontend over a portable-pty bridge
 * exposed as Tauri commands (`pty_spawn`, `pty_write`, `pty_read` events).
 * Placed now so the shell layout is final and the panel contract is visible.
 */
export function TerminalView() {
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-10 shrink-0 items-center border-b border-sprite-border px-3">
        <span className="text-[12px] uppercase tracking-wide text-sprite-muted">Terminal</span>
      </div>
      <div
        data-selectable="true"
        className="flex-1 p-3 font-mono text-[12px] leading-relaxed text-sprite-muted"
      >
        Terminal panel — arriving in Phase 2 (xterm.js + portable-pty).
      </div>
    </div>
  );
}
