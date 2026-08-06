import { Component, type ErrorInfo, type ReactNode } from 'react';
import { motion } from 'framer-motion';
import { AlertTriangle, RefreshCw, Home } from 'lucide-react';
import logger from '../utils/logger';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null,
    errorInfo: null,
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error, errorInfo: null };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    logger.error('Error caught by boundary:', error, errorInfo);
    this.setState({ errorInfo });
  }

  private handleReset = () => {
    this.setState({ hasError: false, error: null, errorInfo: null });
    window.location.reload();
  };

  private handleGoHome = () => {
    window.location.href = '/';
  };

  public render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <div className="flex min-h-screen items-center justify-center bg-paper p-4">
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            className="max-w-lg w-full rounded-2xl border border-line bg-surface p-8 text-center shadow-card"
          >
            <motion.div
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              transition={{ type: 'spring', delay: 0.2 }}
              className="w-20 h-20 bg-red-500/20 rounded-full flex items-center justify-center mx-auto mb-6"
            >
              <AlertTriangle className="w-10 h-10 text-danger" aria-hidden="true" />
            </motion.div>

            <h1 className="font-display text-2xl font-bold text-ink mb-2">
              Something went wrong
            </h1>
            
            <p className="text-muted mb-6">
              We apologize for the inconvenience. An unexpected error has occurred.
            </p>

            {import.meta.env.DEV && this.state.error && (
              <div className="mb-6 text-left">
                <details className="rounded-xl border border-line bg-primary-50/40 p-4">
                  <summary className="cursor-pointer text-sm text-ink font-medium">
                    Error Details (Development Only)
                  </summary>
                  <pre className="mt-3 text-xs text-danger overflow-auto max-h-40">
                    {this.state.error.toString()}
                    {this.state.errorInfo?.componentStack}
                  </pre>
                </details>
              </div>
            )}

            <div className="flex flex-col sm:flex-row gap-3 justify-center">
              <button
                onClick={this.handleReset}
                className="inline-flex items-center justify-center gap-2 px-6 py-3 bg-primary-600 hover:bg-primary-700 text-white rounded-xl font-semibold transition-colors"
                aria-label="Try again by refreshing the page"
              >
                <RefreshCw className="w-4 h-4" aria-hidden="true" />
                Try Again
              </button>
              
              <button
                onClick={this.handleGoHome}
                className="inline-flex items-center justify-center gap-2 px-6 py-3 border border-line2 bg-surface text-ink hover:border-ink/30 rounded-xl font-medium transition-colors"
                aria-label="Go back to homepage"
              >
                <Home className="w-4 h-4" aria-hidden="true" />
                Go Home
              </button>
            </div>
          </motion.div>
        </div>
      );
    }

    return this.props.children;
  }
}

export default ErrorBoundary;

