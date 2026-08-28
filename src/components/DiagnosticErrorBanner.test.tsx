// @vitest-environment jsdom

import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../api/tauri";
import { getMessages } from "../i18n";
import type { CommandError, SafeDiagnostic } from "../types";
import { DiagnosticErrorBanner } from "./DiagnosticErrorBanner";

vi.mock("../api/tauri", () => ({
  invokeCommand: vi.fn(),
}));

const error: CommandError = {
  code: "state_unavailable",
  operation: "capture_clipboard",
  retryable: true,
};

const diagnostic: SafeDiagnostic = {
  timestamp: 1,
  version: "test",
  platform: "test",
  architecture: "test",
  ...error,
};

afterEach(() => {
  vi.clearAllMocks();
});

describe("DiagnosticErrorBanner", () => {
  it("keeps safe diagnostic data out of the release presentation", async () => {
    const container = document.createElement("div");
    const root = createRoot(container);

    flushSync(() => {
      root.render(
        <DiagnosticErrorBanner
          error={error}
          messages={getMessages("zh-CN")}
          onDismiss={vi.fn()}
          showSafeDiagnosticDetails={false}
        />
      );
    });
    await Promise.resolve();

    expect(container.textContent).toContain("无法保存此剪贴板内容。");
    expect(container.querySelector("details")).toBeNull();
    expect(invokeCommand).not.toHaveBeenCalled();

    flushSync(() => root.unmount());
  });

  it("loads safe diagnostic data for the development presentation", async () => {
    vi.mocked(invokeCommand).mockResolvedValue([diagnostic]);
    const container = document.createElement("div");
    const root = createRoot(container);

    flushSync(() => {
      root.render(
        <DiagnosticErrorBanner
          error={error}
          messages={getMessages("zh-CN")}
          onDismiss={vi.fn()}
          showSafeDiagnosticDetails
        />
      );
    });

    await vi.waitFor(() => {
      expect(invokeCommand).toHaveBeenCalledWith(
        "get_safe_diagnostics",
        "capture_clipboard"
      );
      expect(container.querySelector("details")).not.toBeNull();
    });

    flushSync(() => root.unmount());
  });
});
