import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Last-resort error boundary: instead of an opaque black screen (empty #root
 * over a dark body), render the error so failures are visible and debuggable.
 */
export class ErrorBoundary extends Component<Props, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    // eslint-disable-next-line no-console
    console.error("[ui] crashed:", error, info.componentStack);
  }

  override render(): ReactNode {
    if (!this.state.error) {
      return this.props.children;
    }
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 bg-sprite-bg p-8 text-center">
        <h1 className="text-[16px] font-semibold text-sprite-danger">
          Une erreur est survenue dans l'interface
        </h1>
        <pre
          data-selectable="true"
          className="max-w-xl overflow-auto rounded-lg border border-sprite-border bg-sprite-surface p-3 text-left text-[12px] text-sprite-muted"
        >
          {this.state.error.message}
        </pre>
        <button
          type="button"
          onClick={() => this.setState({ error: null })}
          className="rounded-lg bg-sprite-accent px-4 py-2 text-[13px] font-medium text-white hover:bg-sprite-accent-hover"
        >
          Réessayer
        </button>
      </div>
    );
  }
}
