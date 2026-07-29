import { useEffect, useState } from "react";
import { invokeCommand } from "../api/tauri";
import type { Messages } from "../i18n";
import type { CommandError, SafeDiagnostic } from "../types";
import { ErrorBanner } from "./ErrorBanner";

interface DiagnosticErrorBannerProps {
  error: CommandError;
  messages: Messages;
  onDismiss: () => void;
  onRetry?: () => void;
}

interface DiagnosticStatus {
  message: string;
  isError: boolean;
}

export function DiagnosticErrorBanner({
  error,
  messages,
  onDismiss,
  onRetry,
}: DiagnosticErrorBannerProps) {
  const [diagnostic, setDiagnostic] = useState<SafeDiagnostic | undefined>();
  const [status, setStatus] = useState<DiagnosticStatus | null>(null);

  useEffect(() => {
    let disposed = false;
    setDiagnostic(undefined);
    setStatus({ message: messages.diagnosticLoading, isError: false });

    void invokeCommand<SafeDiagnostic[]>(
      "get_safe_diagnostics",
      error.operation
    )
      .then(entries => {
        if (disposed) {
          return;
        }
        const matching = [...entries]
          .reverse()
          .find(
            entry =>
              entry.code === error.code && entry.operation === error.operation
          );
        setDiagnostic(matching);
        setStatus(
          matching
            ? null
            : { message: messages.diagnosticUnavailable, isError: true }
        );
      })
      .catch(() => {
        if (!disposed) {
          setStatus({
            message: messages.diagnosticUnavailable,
            isError: true,
          });
        }
      });

    return () => {
      disposed = true;
    };
  }, [
    error.code,
    error.operation,
    error.retryable,
    messages.diagnosticLoading,
    messages.diagnosticUnavailable,
  ]);

  const copyDiagnostic = async (value: SafeDiagnostic) => {
    try {
      await navigator.clipboard.writeText(JSON.stringify(value, null, 2));
      setStatus({ message: messages.diagnosticCopied, isError: false });
    } catch {
      setStatus({ message: messages.diagnosticCopyFailed, isError: true });
    }
  };

  return (
    <ErrorBanner
      copyDiagnosticLabel={messages.copyDiagnostic}
      diagnostic={diagnostic}
      diagnosticLabel={messages.diagnosticDetails}
      diagnosticStatus={status}
      dismissLabel={messages.dismiss}
      error={error}
      message={messages.commandError(error.operation, error.code)}
      onCopyDiagnostic={copyDiagnostic}
      onDismiss={onDismiss}
      onRetry={onRetry}
      retryLabel={messages.retry}
    />
  );
}
