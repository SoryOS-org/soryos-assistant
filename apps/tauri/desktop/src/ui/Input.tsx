import type { InputHTMLAttributes } from "react";

/** Styled text input primitive. */
export function Input({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={`w-full rounded-md border border-sprite-border bg-sprite-bg px-2.5 py-1.5 text-[13px] text-sprite-text outline-none placeholder:text-sprite-muted focus:border-sprite-accent ${className}`}
      {...rest}
    />
  );
}
