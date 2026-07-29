import { invoke } from "@tauri-apps/api/core";
import type {
  CommandError,
  ErrorCode,
  Operation,
  SafeDiagnostic,
} from "../types";

const errorCodes = new Set<ErrorCode>([
  "startup_failed",
  "database_unavailable",
  "database_operation_failed",
  "history_item_not_found",
  "clipboard_write_failed",
  "restore_post_processing_failed",
  "invalid_setting",
  "invalid_history_cursor",
  "state_unavailable",
  "autostart_unavailable",
  "autostart_verification_failed",
  "history_mirror_failed",
  "capture_rejected",
]);

const operations = new Set<Operation>([
  "startup",
  "capture_clipboard",
  "load_history",
  "load_history_detail",
  "restore_clipboard",
  "delete_history",
  "clear_history",
  "load_settings",
  "update_settings",
  "update_autostart",
  "write_history_mirror",
]);

export class TauriCommandError extends Error implements CommandError {
  readonly code: ErrorCode;
  readonly operation: Operation;
  readonly retryable: boolean;

  constructor(error: CommandError) {
    super(`${error.code}:${error.operation}`);
    this.name = "TauriCommandError";
    this.code = error.code;
    this.operation = error.operation;
    this.retryable = error.retryable;
  }
}

export function normalizeCommandError(
  value: unknown,
  fallbackOperation: Operation
): TauriCommandError {
  if (value instanceof TauriCommandError) {
    return value;
  }

  if (typeof value === "object" && value !== null) {
    const candidate = value as Partial<CommandError>;
    const code =
      typeof candidate.code === "string" &&
      errorCodes.has(candidate.code as ErrorCode)
        ? (candidate.code as ErrorCode)
        : "unknown";
    const operation =
      typeof candidate.operation === "string" &&
      operations.has(candidate.operation as Operation)
        ? (candidate.operation as Operation)
        : fallbackOperation;
    return new TauriCommandError({
      code,
      operation,
      retryable: candidate.retryable === true,
    });
  }

  return new TauriCommandError({
    code: "unknown",
    operation: fallbackOperation,
    retryable: true,
  });
}

export async function invokeCommand<T>(
  command: string,
  operation: Operation,
  args?: Record<string, unknown>
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeCommandError(error, operation);
  }
}

export function diagnosticPreview(
  error: CommandError,
  metadata: Pick<SafeDiagnostic, "version" | "platform" | "architecture"> = {
    version: "frontend",
    platform:
      typeof navigator === "undefined"
        ? "unknown"
        : navigator.platform || "unknown",
    architecture: "unknown",
  }
): SafeDiagnostic {
  return {
    timestamp: Date.now(),
    ...metadata,
    code: error.code,
    operation: error.operation,
    retryable: error.retryable,
  };
}
