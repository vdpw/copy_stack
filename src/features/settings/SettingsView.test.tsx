// @vitest-environment jsdom
/* global HTMLButtonElement, HTMLInputElement */

import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { invokeCommand, TauriCommandError } from "../../api/tauri";
import type { AppSettingsController } from "../../hooks/useAppSettings";
import { getMessages } from "../../i18n";
import type { AppSettings } from "../../types";
import { SettingsView } from "./SettingsView";

vi.mock("../../api/tauri", async importOriginal => ({
  ...(await importOriginal<typeof import("../../api/tauri")>()),
  invokeCommand: vi.fn().mockResolvedValue([]),
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
  theme: "system",
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
    updateTheme: vi.fn(),
    updateAutostart: vi.fn(),
    reportError: vi.fn(),
    retryError: vi.fn(),
    dismissError: vi.fn(),
  };
}

const cleanups: (() => void)[] = [];

function renderSettings(controller = createController()) {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  const onBack = vi.fn();
  const render = () => {
    flushSync(() => {
      root.render(
        <SettingsView
          controller={controller}
          language="zh-CN"
          messages={getMessages("zh-CN")}
          onBack={onBack}
        />
      );
    });
  };
  render();
  cleanups.push(() => {
    flushSync(() => root.unmount());
    container.remove();
  });
  const selectCategory = (name: string) => {
    const button = Array.from(
      container.querySelectorAll<HTMLButtonElement>("nav button")
    ).find(item => item.textContent === name);
    expect(button).toBeDefined();
    flushSync(() => button?.click());
  };
  return { container, controller, onBack, render, selectCategory };
}

function editInput(container: HTMLElement, selector: string, value: string) {
  const input = container.querySelector<HTMLInputElement>(selector)!;
  flushSync(() => {
    Object.getOwnPropertyDescriptor(
      window.HTMLInputElement.prototype,
      "value"
    )?.set?.call(input, value);
    input.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
  return input;
}

afterEach(() => {
  cleanups.splice(0).forEach(cleanup => cleanup());
  vi.clearAllMocks();
});

describe("SettingsView", () => {
  it("groups settings into four pages and always keeps the back action available", () => {
    const { container, selectCategory, onBack } = renderSettings();
    const content = () => container.querySelector("main")?.textContent;

    expect(container.querySelector("h1")?.textContent).toBe("通用");
    expect(
      container.querySelector('nav [aria-current="page"]')?.textContent
    ).toBe("通用");
    expect(content()).toContain("语言");
    expect(content()).toContain("登录时启动");
    expect(container.querySelector("#max-items-input")).toBeNull();
    expect(container.querySelector('input[type="radio"]')).toBeNull();
    expect(container.querySelector("#menu-bar-item-limit-input")).toBeNull();

    selectCategory("外观");
    expect(container.querySelector("h1")?.textContent).toBe("外观");
    expect(
      Array.from(
        container.querySelectorAll<HTMLInputElement>('input[type="radio"]'),
        input => input.value
      )
    ).toEqual(["light", "dark", "system"]);
    const themeGroup = container.querySelector("fieldset");
    expect(themeGroup?.getAttribute("aria-labelledby")).toBe(
      "appearance-theme-title"
    );
    expect(themeGroup?.getAttribute("aria-describedby")).toBeNull();
    expect(
      container.querySelector("#appearance-theme-title")?.textContent
    ).toBe(getMessages("zh-CN").theme);
    expect(container.querySelector("#appearance-theme-description")).toBeNull();
    expect(container.querySelector("#language-select")).toBeNull();
    expect(container.querySelector("#menu-bar-item-limit-input")).toBeNull();

    selectCategory("剪贴板");
    expect(content()).toContain(getMessages("zh-CN").compactMode);
    expect(content()).toContain(getMessages("zh-CN").moveRestoredItemsToTop);
    expect(container.querySelector("#max-items-input")).not.toBeNull();
    expect(container.querySelector("#history-budget-input")).not.toBeNull();
    expect(content()).toContain("清空未固定");
    const page = container.querySelector(".settings-page-scroll")!;
    page.scrollTop = 300;

    selectCategory("菜单栏");
    expect(page.scrollTop).toBe(0);
    expect(content()).toContain(getMessages("zh-CN").showInMenuBar);
    expect(
      container.querySelector("#menu-bar-item-limit-input")
    ).not.toBeNull();
    expect(container.querySelector("#max-items-input")).toBeNull();
    expect(
      container.querySelectorAll('nav [aria-current="page"]')
    ).toHaveLength(1);
    expect(
      container.querySelector('nav [aria-current="page"]')?.textContent
    ).toBe("菜单栏");

    container
      .querySelector<HTMLButtonElement>(".settings-sidebar-back")
      ?.click();
    expect(onBack).toHaveBeenCalledOnce();
  });

  it.each(["system", "light", "dark"] as const)(
    "saves the selected %s theme",
    theme => {
      const controller = createController();
      controller.settings = {
        ...settings,
        theme: theme === "system" ? "dark" : "system",
      };
      const { container, selectCategory, render } = renderSettings(controller);
      selectCategory("外观");
      const radio = container.querySelector<HTMLInputElement>(
        `input[value="${theme}"]`
      )!;

      flushSync(() => radio.click());
      expect(controller.updateTheme).toHaveBeenCalledExactlyOnceWith(theme);
      controller.settings = { ...settings, theme };
      render();
      expect(radio.checked).toBe(true);
      expect(
        container.querySelectorAll('input[type="radio"]:checked')
      ).toHaveLength(1);
    }
  );

  it("disables theme changes while a setting is being saved", () => {
    const controller = createController();
    controller.updating = true;
    const { container, selectCategory } = renderSettings(controller);
    selectCategory("外观");
    const radios = container.querySelectorAll<HTMLInputElement>(
      'input[type="radio"]'
    );
    expect(Array.from(radios).every(input => input.disabled)).toBe(true);
    Array.from(radios)
      .find(input => input.value === "dark")
      ?.click();
    expect(controller.updateTheme).not.toHaveBeenCalled();
  });

  it.each([false, true])(
    "preserves navigation when settings are unavailable (error: %s)",
    hasError => {
      const controller = createController();
      controller.settings = null;
      controller.loading = !hasError;
      if (hasError) {
        controller.error = new TauriCommandError({
          code: "database_unavailable",
          operation: "load_settings",
          retryable: true,
        });
      }
      const { container, selectCategory, onBack } = renderSettings(controller);
      expect(container.querySelectorAll("nav button")).toHaveLength(4);
      selectCategory("外观");
      expect(container.querySelector("h1")?.textContent).toBe("外观");
      expect(container.querySelector('input[type="radio"]')).toBeNull();
      if (hasError) {
        expect(
          container.querySelector('[role="alert"]')?.textContent
        ).toContain("无法加载设置");
      } else {
        expect(container.querySelector('[role="status"]')?.textContent).toBe(
          "正在加载设置..."
        );
      }
      container
        .querySelector<HTMLButtonElement>(".settings-sidebar-back")
        ?.click();
      expect(onBack).toHaveBeenCalledOnce();
    }
  );

  it("shows concise storage help on demand without changing a setting", () => {
    const { container, controller, selectCategory } = renderSettings();
    selectCategory("剪贴板");
    const helpButtons = container.querySelectorAll<HTMLButtonElement>(
      ".settings-help-trigger"
    );
    expect(helpButtons).toHaveLength(2);
    expect(container.querySelector('[role="note"]')).toBeNull();
    expect(helpButtons[0].getAttribute("aria-label")).toBe("存储数量说明");
    expect(helpButtons[0].getAttribute("aria-expanded")).toBe("false");

    flushSync(() => helpButtons[0].click());
    expect(helpButtons[0].getAttribute("aria-expanded")).toBe("true");
    expect(helpButtons[0].getAttribute("aria-controls")).toBe(
      "stored-items-help"
    );
    expect(helpButtons[0].getAttribute("aria-describedby")).toBe(
      "stored-items-help"
    );
    expect(container.querySelector('[role="note"]')?.textContent).toBe(
      getMessages("zh-CN").storedItemsHelp
    );
    flushSync(() => helpButtons[0].click());
    expect(container.querySelector('[role="note"]')).toBeNull();

    flushSync(() => helpButtons[1].click());
    expect(container.querySelector('[role="note"]')?.textContent).toContain(
      "单条最多 8 MB"
    );
    expect(controller.updateMaxItems).not.toHaveBeenCalled();
    expect(controller.updateMaxHistoryBytes).not.toHaveBeenCalled();
  });

  it("dismisses storage help on outside click, Escape, and category navigation", () => {
    const { container, selectCategory } = renderSettings();
    selectCategory("剪贴板");
    const trigger = container.querySelector<HTMLButtonElement>(
      ".settings-help-trigger"
    )!;
    flushSync(() => trigger.click());
    flushSync(() =>
      container.querySelector<HTMLInputElement>("#max-items-input")?.click()
    );
    expect(container.querySelector('[role="note"]')).toBeNull();

    trigger.focus();
    flushSync(() => trigger.click());
    flushSync(() =>
      trigger.dispatchEvent(
        new window.KeyboardEvent("keydown", {
          key: "Escape",
          bubbles: true,
          cancelable: true,
        })
      )
    );
    expect(container.querySelector('[role="note"]')).toBeNull();
    expect(document.activeElement).toBe(trigger);

    flushSync(() => trigger.click());
    selectCategory("外观");
    selectCategory("剪贴板");
    expect(container.querySelector('[role="note"]')).toBeNull();
    expect(
      container
        .querySelector(".settings-help-trigger")
        ?.getAttribute("aria-expanded")
    ).toBe("false");
  });

  it("preserves unapplied input drafts across category and unrelated setting changes", async () => {
    const { container, controller, selectCategory, render } = renderSettings();
    selectCategory("剪贴板");
    editInput(container, "#max-items-input", "250");
    editInput(container, "#history-budget-input", "512");
    selectCategory("菜单栏");
    editInput(container, "#menu-bar-item-limit-input", "20");
    selectCategory("外观");
    controller.settings = { ...settings, theme: "dark", history_count: 13 };
    render();

    selectCategory("剪贴板");
    expect(
      container.querySelector<HTMLInputElement>("#max-items-input")?.value
    ).toBe("250");
    expect(
      container.querySelector<HTMLInputElement>("#history-budget-input")?.value
    ).toBe("512");
    selectCategory("菜单栏");
    expect(
      container.querySelector<HTMLInputElement>("#menu-bar-item-limit-input")
        ?.value
    ).toBe("20");

    controller.settings = { ...settings, max_items: 300 };
    render();
    selectCategory("剪贴板");
    await vi.waitFor(() => {
      expect(
        container.querySelector<HTMLInputElement>("#max-items-input")?.value
      ).toBe("300");
    });
    expect(
      container.querySelector<HTMLInputElement>("#history-budget-input")?.value
    ).toBe("512");
  });

  it("contains confirmation focus and restores it when Escape cancels clearing", () => {
    const { container, selectCategory } = renderSettings();
    selectCategory("剪贴板");
    const trigger = container.querySelector<HTMLButtonElement>(
      ".settings-clear-button"
    )!;
    flushSync(() => trigger.click());
    const dialog = container.querySelector<HTMLElement>('[role="dialog"]')!;
    const [cancel, confirm] =
      dialog.querySelectorAll<HTMLButtonElement>("button");
    expect(document.activeElement).toBe(cancel);
    expect(
      container.querySelector(".settings-shell")?.hasAttribute("inert")
    ).toBe(true);

    cancel.dispatchEvent(
      new window.KeyboardEvent("keydown", {
        key: "Tab",
        shiftKey: true,
        bubbles: true,
        cancelable: true,
      })
    );
    expect(document.activeElement).toBe(confirm);
    confirm.dispatchEvent(
      new window.KeyboardEvent("keydown", {
        key: "Tab",
        bubbles: true,
        cancelable: true,
      })
    );
    expect(document.activeElement).toBe(cancel);

    flushSync(() =>
      cancel.dispatchEvent(
        new window.KeyboardEvent("keydown", {
          key: "Escape",
          bubbles: true,
          cancelable: true,
        })
      )
    );
    expect(container.querySelector('[role="dialog"]')).toBeNull();
    expect(
      container.querySelector(".settings-shell")?.hasAttribute("inert")
    ).toBe(false);
    expect(document.activeElement).toBe(trigger);
    expect(invokeCommand).not.toHaveBeenCalled();
  });

  it("cancels a retention reduction with Escape and returns focus to its input", () => {
    const { container, controller, selectCategory } = renderSettings();
    selectCategory("剪贴板");
    const input = editInput(container, "#max-items-input", "5");
    const apply = input
      .closest(".preference-row")!
      .querySelector<HTMLButtonElement>("button.btn-primary")!;
    flushSync(() => apply.click());
    const cancel = container.querySelector<HTMLButtonElement>(
      "[data-dialog-cancel]"
    )!;
    expect(document.activeElement).toBe(cancel);
    flushSync(() =>
      cancel.dispatchEvent(
        new window.KeyboardEvent("keydown", {
          key: "Escape",
          bubbles: true,
          cancelable: true,
        })
      )
    );
    expect(container.querySelector('[role="dialog"]')).toBeNull();
    expect(input.value).toBe("100");
    expect(document.activeElement).toBe(input);
    expect(controller.updateMaxItems).not.toHaveBeenCalled();
  });

  it("asks for confirmation before clearing unpinned clipboard history", async () => {
    const { container, controller, selectCategory } = renderSettings();
    selectCategory("剪贴板");
    const clearButton = Array.from(container.querySelectorAll("button")).find(
      button => button.textContent?.includes("清空未固定")
    );
    expect(clearButton).toBeDefined();
    flushSync(() => clearButton?.click());

    expect(invokeCommand).not.toHaveBeenCalled();
    const dialog = container.querySelector('[role="dialog"]');
    expect(dialog?.textContent).toContain("清空未固定的剪贴板历史？");
    expect(dialog?.textContent).toContain("固定项目会保留");
    const confirmButton = Array.from(
      container.querySelectorAll<HTMLElement>('[role="dialog"] button')
    ).find(button => button.textContent?.includes("清空未固定"));
    expect(confirmButton).toBeDefined();
    confirmButton?.click();

    await vi.waitFor(() => {
      expect(invokeCommand).toHaveBeenCalledWith(
        "clear_all_events",
        "clear_history"
      );
      expect(controller.loadSettings).toHaveBeenCalledOnce();
      expect(document.activeElement).toBe(clearButton);
    });
  });
});
