import type { ReactNode } from "react";
import { Button } from "@/ui/Button";

interface ModalProps {
  open: boolean;
  title: string;
  children: ReactNode;
  confirmLabel?: string;
  danger?: boolean;
  onConfirm?: () => void;
  onCancel: () => void;
}

/**
 * Confirmation dialog primitive.
 * Phase 2 target: rendered from Codex `execCommandApproval` / `applyPatchApproval`
 * requests (finally wiring the dormant `confirmation_required` concept).
 */
export function Modal({
  open,
  title,
  children,
  confirmLabel = "Confirm",
  danger = false,
  onConfirm,
  onCancel,
}: ModalProps) {
  if (!open) {
    return null;
  }

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={onCancel}
    >
      <div
        className="w-[420px] rounded-lg border border-sprite-border bg-sprite-surface p-4 shadow-2xl"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 className="mb-2 text-[15px] font-semibold text-sprite-text">{title}</h2>
        <div className="mb-4 text-[13px] leading-relaxed text-sprite-muted" data-selectable="true">
          {children}
        </div>
        <div className="flex justify-end gap-2">
          <Button onClick={onCancel}>Cancel</Button>
          <Button
            variant={danger ? "danger" : "primary"}
            onClick={() => onConfirm?.()}
          >
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
