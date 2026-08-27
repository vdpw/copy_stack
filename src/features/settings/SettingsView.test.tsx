// @vitest-environment jsdom

import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../api/tauri";
import type { AppSettingsController } from "../../hooks/useAppSettings";
import { getMessages } from "../../i18n";
import type { AppSettings } from "../../types";
import { SettingsView } from "./SettingsView";

vi.mock("../../api/tauri", () => ({
  invokeCommand: vi.fn(),
}));

const settings: AppSettings = {
  max_items: 100,
  max_history_bytes: 256 * 1024 * 1024,
  show_in_menu_bar: true,
  menu_bar_item_limit: 0,
  move_restored_item_to_top: false,
  compact_mode: false,
  language: "zh-CN",
  resolved_language: "zh-CN",
  history_count: 12,
  history_bytes: 1024,
  history_limit_bytes: 256 * 1024 * 1024,
  max_event_bytes: 8 * 1024 * 1024,
};

function createController(): AppSettingsController {
  return {
    settings,
    loading: false,
    updating: false,
    error: null,
    autostartEnabled: false,
    autostartLoading: false,
    autostartError: null,
    loadSettings: vi.fn().mockResolvedValue(settings),
    updateMaxItems: vi.fn(),
    updateMaxHistoryBytes: vi.fn(),
    updateMenuBarVisibility: vi.fn(),
    updateMenuBarItemLimit: vi.fn(),
    updateRestoreOrdering: vi.fn(),
    updateCompactMode: vi.fn(),
    updateLanguage: vi.fn(),
    updateAutostart: vi.fn(),
    reportError: vi.fn(),
    retryError: vi.fn(),
    dismissError: vi.fn(),
  };
}

afterEach(() => {
  vi.clearAllMocks();
});

describe("SettingsView", () => {
  it("asks for confirmation before clearing clipboard history", async () => {
    const container = document.createElement("div");
    const root = createRoot(container);
    const controller = createController();

    flushSync(() => {
      root.render(
        <SettingsView
          controller={controller}
          language="zh-CN"
          messages={getMessages("zh-CN")}
          onBack={vi.fn()}
        />
      );
    });

    const clearButton = Array.from(container.querySelectorAll("button")).find(
      button => button.textContent?.includes("全部清空")
    );
    expect(clearButton).toBeDefined();

    flushSync(() => clearButton?.click());

    expect(invokeCommand).not.toHaveBeenCalled();
    expect(container.querySelector('[role="dialog"]')?.textContent).toContain(
      "清空全部剪贴板历史？"
    );

    const confirmButton = Array.from(
      container.querySelectorAll<HTMLElement>('[role="dialog"] button')
    ).find(button => button.textContent?.includes("全部清空"));
    expect(confirmButton).toBeDefined();
    confirmButton?.click();

    await vi.waitFor(() => {
      expect(invokeCommand).toHaveBeenCalledWith(
        "clear_all_events",
        "clear_history"
      );
    });

    flushSync(() => root.unmount());
  });
});
