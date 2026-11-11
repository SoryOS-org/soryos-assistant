import type { ButtonHTMLAttributes, ReactNode } from "react";

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Short label used for the tooltip / accessibility. */
  label: string;
  children: ReactNode;
}

/** Square icon-only button with tooltip, used across toolbars and rails. */
export function IconButton({ label, className = "", children, ...rest }: IconButtonProps) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      className={`inline-flex h-7 w-7 items-center justify-center rounded-md text-sprite-muted transition-colors hover:bg-surface-raised hover:text-sprite-text disabled:pointer-events-none disabled:opacity-40 ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
