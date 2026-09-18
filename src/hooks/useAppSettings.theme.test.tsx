// @vitest-environment jsdom

import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../types";
import { useAppSettings } from "./useAppSettings";
import { useAppearance } from "./useAppearance";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const initial: AppSettings = {
  theme: "system",
  max_items: 100,
  max_history_bytes: 268435456,
  show_in_menu_bar: true,
  menu_bar_item_limit: 0,
  move_restored_item_to_top: false,
  compact_mode: false,
  language: "en",
  resolved_language: "en",
  history_count: 0,
  history_bytes: 0,
  history_limit_bytes: 268435456,
  max_event_bytes: 33554432,
};
const roots: ReturnType<typeof createRoot>[] = [];
afterEach(() => {
  roots.splice(0).forEach(root => flushSync(() => root.unmount()));
  delete document.documentElement.dataset.theme;
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

function Harness() {
  const controller = useAppSettings(false);
  useAppearance(controller.settings?.theme ?? "system");
  return (
    <>
      <output>{controller.settings?.theme}</output>
      <button
        disabled={!controller.settings || controller.updating}
        onClick={() => void controller.updateTheme("dark")}
      >
        Dark
      </button>
      {controller.error && (
        <span role="alert">{controller.error.operation}</span>
      )}
    </>
  );
}

it.each([true, false])(
  "updates appearance immediately and reconciles persisted state (success=%s)",
  async success => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn(() => ({
        matches: false,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      }))
    );
    let stored = initial;
    let resolveMutation: () => void = () => {};
    let rejectMutation: (error: unknown) => void = () => {};
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_app_settings") return stored;
      if (command === "set_theme") {
        expect(args).toEqual({ theme: "dark" });
        await new Promise<void>((resolve, reject) => {
          resolveMutation = resolve;
          rejectMutation = reject;
        });
        stored = { ...initial, theme: "dark" };
        return undefined;
      }
      throw new Error("unexpected test command");
    });
    const container = document.createElement("div");
    const root = createRoot(container);
    roots.push(root);
    flushSync(() => root.render(<Harness />));
    await vi.waitFor(() =>
      expect(container.querySelector("output")?.textContent).toBe("system")
    );
    flushSync(() => container.querySelector("button")?.click());
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(container.querySelector("button")?.disabled).toBe(true);
    if (success) resolveMutation();
    else
      rejectMutation({
        code: "database_operation_failed",
        operation: "update_settings",
        retryable: true,
      });
    await vi.waitFor(() =>
      expect(container.querySelector("button")?.disabled).toBe(false)
    );
    expect(container.querySelector("output")?.textContent).toBe(
      success ? "dark" : "system"
    );
    expect(document.documentElement.dataset.theme).toBe(
      success ? "dark" : "light"
    );
    if (!success)
      expect(container.querySelector('[role="alert"]')?.textContent).toBe(
        "update_settings"
      );
  }
);
