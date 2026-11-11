/**
 * FilesView — workspace file explorer panel.
 *
 * Phase 2 implementation plan: scoped Tauri commands (`fs_list`, `fs_read`)
 * restricted to the active workspace root, tree with lazy folder expansion,
 * and file preview feeding the chat context.
 */
export function FilesView() {
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-10 shrink-0 items-center border-b border-sprite-border px-3">
        <span className="text-[12px] uppercase tracking-wide text-sprite-muted">Files</span>
      </div>
      <div className="flex-1 p-3 text-[13px] text-sprite-muted">
        File explorer panel — arriving in Phase 2 (scoped fs commands).
      </div>
    </div>
  );
}
