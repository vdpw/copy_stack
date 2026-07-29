import { describe, expect, it } from "vitest";
import { diagnosticPreview, normalizeCommandError } from "./tauri";

describe("normalizeCommandError", () => {
  it("preserves only known structured error fields", () => {
    const error = normalizeCommandError(
      {
        code: "database_operation_failed",
        operation: "load_history",
        retryable: true,
        content_hash: "must-not-escape",
        path: "/private/example",
      },
      "load_settings"
    );

    expect(error).toMatchObject({
      code: "database_operation_failed",
      operation: "load_history",
      retryable: true,
    });
    expect(JSON.stringify(error)).not.toContain("must-not-escape");
    expect(JSON.stringify(error)).not.toContain("/private/example");
  });

  it("maps raw internal errors to a safe fallback", () => {
    const error = normalizeCommandError(
      "sqlite failed for /private/path with clipboard body",
      "load_history"
    );

    expect(error).toMatchObject({
      code: "unknown",
      operation: "load_history",
      retryable: true,
    });
    expect(error.message).not.toContain("private/path");
  });
});

describe("diagnosticPreview", () => {
  it("contains only safe operation metadata", () => {
    const diagnostic = diagnosticPreview(
      {
        code: "clipboard_write_failed",
        operation: "restore_clipboard",
        retryable: true,
      },
      {
        version: "0.1.0-test",
        platform: "test",
        architecture: "test",
      }
    );

    expect(diagnostic).toMatchObject({
      code: "clipboard_write_failed",
      operation: "restore_clipboard",
      version: "0.1.0-test",
    });
    expect(JSON.stringify(diagnostic)).not.toMatch(
      /content_hash|source_bundle|event_data|file:\/\//
    );
  });
});
