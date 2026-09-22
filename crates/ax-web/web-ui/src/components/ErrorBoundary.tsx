import { Component, type ErrorInfo, type ReactNode } from 'react';

/** Keeps Command Center chrome visible when a page throws (e.g. nested policy JSON). */
export class ErrorBoundary extends Component<
  { children: ReactNode; fallbackTitle?: string },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="page-empty" role="alert">
          <h2>{this.props.fallbackTitle ?? 'This page failed to render'}</h2>
          <p className="muted">{this.state.error.message}</p>
          <button type="button" className="policy-link" onClick={() => this.setState({ error: null })}>
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
