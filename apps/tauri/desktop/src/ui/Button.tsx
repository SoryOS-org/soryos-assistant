import type { ButtonHTMLAttributes, ReactNode } from "react";

type Variant = "primary" | "ghost" | "danger";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  children: ReactNode;
}

const variantClasses: Record<Variant, string> = {
  primary:
    "bg-sprite-accent text-white hover:bg-sprite-accent-hover active:translate-y-px",
  ghost:
    "bg-transparent text-sprite-muted hover:bg-surface-raised hover:text-sprite-text",
  danger:
    "bg-transparent text-sprite-danger hover:bg-sprite-danger/10",
};

/** Base button primitive. */
export function Button({ variant = "ghost", className = "", children, ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      className={`inline-flex items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-[13px] font-medium transition-colors disabled:pointer-events-none disabled:opacity-40 ${variantClasses[variant]} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
