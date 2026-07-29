import { AlertTriangle, Copy, RefreshCw, X } from "lucide-react";
import type { CommandError, SafeDiagnostic } from "../types";

interface ErrorBannerProps {
  error: CommandError;
  message: string;
  diagnosticLabel: string;
  copyDiagnosticLabel: string;
  dismissLabel: string;
  retryLabel: string;
  onDismiss: () => void;
  onRetry?: () => void;
  onCopyDiagnostic?: (diagnostic: SafeDiagnostic) => Promise<void> | void;
  diagnostic?: SafeDiagnostic;
  diagnosticStatus?: {
    message: string;
    isError: boolean;
  } | null;
}

export function ErrorBanner({
  error,
  message,
  diagnosticLabel,
  copyDiagnosticLabel,
  dismissLabel,
  retryLabel,
  onDismiss,
  onRetry,
  onCopyDiagnostic,
  diagnostic,
  diagnosticStatus,
}: ErrorBannerProps) {
  return (
    <aside aria-live="assertive" className="error-banner" role="alert">
      <AlertTriangle aria-hidden="true" size={20} />
      <div className="error-banner-content">
        <p>{message}</p>
        {diagnostic && (
          <details>
            <summary>{diagnosticLabel}</summary>
            <pre>{JSON.stringify(diagnostic, null, 2)}</pre>
            {onCopyDiagnostic && (
              <button
                className="btn btn-secondary"
                onClick={() => void onCopyDiagnostic(diagnostic)}
                type="button"
              >
                <Copy aria-hidden="true" size={14} />
                {copyDiagnosticLabel}
              </button>
            )}
          </details>
        )}
        {diagnosticStatus && (
          <p
            className={
              diagnosticStatus.isError
                ? "diagnostic-status diagnostic-status-error"
                : "diagnostic-status"
            }
            role={diagnosticStatus.isError ? "alert" : "status"}
          >
            {diagnosticStatus.message}
          </p>
        )}
      </div>
      {error.retryable && onRetry && (
        <button
          aria-label={retryLabel}
          className="error-banner-icon-button"
          onClick={onRetry}
          title={retryLabel}
          type="button"
        >
          <RefreshCw aria-hidden="true" size={16} />
        </button>
      )}
      <button
        aria-label={dismissLabel}
        className="error-banner-icon-button"
        onClick={onDismiss}
        title={dismissLabel}
        type="button"
      >
        <X aria-hidden="true" size={16} />
      </button>
    </aside>
  );
}
