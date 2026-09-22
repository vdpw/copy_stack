// @vitest-environment jsdom

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ElementRef } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import type { Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getMessages } from "../../i18n";
import type { SupportedLanguage } from "../../i18n";
import type { HistorySummary } from "../../types";
import { HistoryView } from "./HistoryView";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const targetHash = "pinned-target";
const summary: HistorySummary = {
  content_hash: targetHash,
  data_type: "text",
  display: Array.from(new globalThis.TextEncoder().encode("Pinned example")),
  display_truncated: false,
  source_bundle_id: null,
  is_remote_clipboard: false,
  is_pinned: true,
  timestamp: 0,
  byte_count: 14,
  has_detail: false,
  search_preview: null,
};

let container: HTMLElement;
let root: Root;
let storedItems: HistorySummary[];
const messages = getMessages("en");
const onHistoryChanged = vi.fn<() => Promise<void>>();
const deleteCommand = vi.fn<(contentHash: string) => Promise<void>>();

function removeItem(contentHash: string) {
  storedItems = storedItems.filter(item => item.content_hash !== contentHash);
}

function deleteCalls() {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([command]) => command === "delete_copy_event");
}

function deleteButton(language: SupportedLanguage = "en") {
  return container.querySelector<ElementRef<"button">>(
    `[data-history-hash="${targetHash}"] button[aria-label="${getMessages(language).deleteItem}"]`
  )!;
}

function dialogButtons() {
  return container.querySelectorAll<ElementRef<"button">>(
    '[role="dialog"] button'
  );
}

function retryButton() {
  return container.querySelector<ElementRef<"button">>(
    `.error-banner button[aria-label="${messages.retry}"]`
  )!;
}

function key(target: HTMLElement, name: string, shiftKey = false) {
  target.dispatchEvent(
    new window.KeyboardEvent("keydown", {
      key: name,
      shiftKey,
      bubbles: true,
      cancelable: true,
    })
  );
}

async function renderHistory(
  language: SupportedLanguage = "en",
  compactMode = false,
  focusSearchRequest = 0
) {
  flushSync(() => {
    root.render(
      <HistoryView
        compactMode={compactMode}
        focusSearchRequest={focusSearchRequest}
        language={language}
        messages={getMessages(language)}
        moveRestoredItemToTop={false}
        onHistoryChanged={onHistoryChanged}
      />
    );
  });
  await vi.waitFor(() => expect(deleteButton(language)).not.toBeNull());
}

beforeEach(() => {
  vi.clearAllMocks();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  storedItems = [{ ...summary }];
  onHistoryChanged.mockResolvedValue(undefined);
  deleteCommand.mockReset();
  deleteCommand.mockImplementation(async contentHash =>
    removeItem(contentHash)
  );
  vi.mocked(listen).mockResolvedValue(vi.fn());
  vi.spyOn(window, "requestAnimationFrame").mockReturnValue(0);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const argumentsByName = args as Record<string, unknown> | undefined;
    if (command === "get_copy_events_page") {
      const query = String(argumentsByName?.query ?? "").toLowerCase();
      const items = storedItems.filter(item =>
        new TextDecoder()
          .decode(new Uint8Array(item.display))
          .toLowerCase()
          .includes(query)
      );
      return {
        items,
        next_cursor: null,
        has_more: false,
        total_count: items.length,
        total_bytes: 14 * items.length,
      };
    }
    if (command === "delete_copy_event") {
      return deleteCommand(String(argumentsByName?.contentHash));
    }
    if (command === "get_safe_diagnostics") return [];
    throw new Error(`Unexpected command: ${command}`);
  });
});

afterEach(() => {
  flushSync(() => root.unmount());
  container.remove();
  vi.restoreAllMocks();
});

describe("HistoryView deletion", () => {
  it("deletes an unpinned item directly and prevents repeated in-flight commands", async () => {
    storedItems = [{ ...summary, is_pinned: false }];
    let completeDelete!: () => void;
    deleteCommand.mockImplementationOnce(
      contentHash =>
        new Promise(resolve => {
          completeDelete = () => {
            removeItem(contentHash);
            resolve();
          };
        })
    );
    await renderHistory();
    const trigger = deleteButton();

    flushSync(() => {
      trigger.click();
      trigger.click();
    });

    expect(container.querySelector('[role="dialog"]')).toBeNull();
    expect(deleteCalls()).toEqual([
      ["delete_copy_event", { contentHash: targetHash }],
    ]);
    expect(trigger.disabled).toBe(true);
    expect(
      container.querySelector(".event-content")?.getAttribute("aria-expanded")
    ).toBe("false");
    expect(
      container.querySelector<ElementRef<"button">>(
        `button[aria-label="${messages.pinItem}"]`
      )?.disabled
    ).toBe(true);
    completeDelete();
    await vi.waitFor(() => {
      expect(container.querySelector("[data-history-hash]")).toBeNull();
      expect(onHistoryChanged).toHaveBeenCalledOnce();
    });
  });

  it("defaults to Cancel, traps focus and preserves the search when Escape dismisses", async () => {
    await renderHistory();
    const search = container.querySelector<ElementRef<"input">>(
      'input[type="search"]'
    )!;
    flushSync(() => {
      Object.getOwnPropertyDescriptor(
        window.HTMLInputElement.prototype,
        "value"
      )!.set!.call(search, "Pinned");
      search.dispatchEvent(new window.Event("input", { bubbles: true }));
    });
    await vi.waitFor(() => {
      expect(
        vi
          .mocked(invoke)
          .mock.calls.some(
            ([command, args]) =>
              command === "get_copy_events_page" &&
              (args as Record<string, unknown> | undefined)?.query === "Pinned"
          )
      ).toBe(true);
    });
    const trigger = deleteButton();
    trigger.focus();
    flushSync(() => trigger.click());
    const [cancel, confirm] = dialogButtons();
    expect(document.activeElement).toBe(cancel);
    expect(container.querySelector("main")?.hasAttribute("inert")).toBe(true);
    expect(deleteCalls()).toHaveLength(0);
    expect(
      container.querySelector(".event-content")?.getAttribute("aria-expanded")
    ).toBe("false");
    key(cancel, "Tab", true);
    expect(document.activeElement).toBe(confirm);
    key(confirm, "Tab");
    expect(document.activeElement).toBe(cancel);
    window.dispatchEvent(
      new window.KeyboardEvent("keydown", {
        key: "f",
        metaKey: true,
        cancelable: true,
      })
    );
    expect(document.activeElement).toBe(cancel);
    flushSync(() => key(cancel, "Escape"));
    expect(container.querySelector('[role="dialog"]')).toBeNull();
    expect(container.querySelector("main")?.hasAttribute("inert")).toBe(false);
    expect(document.activeElement).toBe(trigger);
    expect(search.value).toBe("Pinned");
    expect(deleteCalls()).toHaveLength(0);
  });

  it("confirms only the selected pinned item once and restores usable focus after deletion", async () => {
    let completeDelete!: () => void;
    deleteCommand.mockImplementationOnce(
      contentHash =>
        new Promise(resolve => {
          completeDelete = () => {
            removeItem(contentHash);
            resolve();
          };
        })
    );
    await renderHistory();
    flushSync(() => deleteButton().click());
    const confirm = dialogButtons()[1];
    flushSync(() => {
      confirm.click();
      confirm.click();
    });
    expect(deleteCalls()).toEqual([
      ["delete_copy_event", { contentHash: targetHash }],
    ]);
    expect(container.querySelector('[role="dialog"]')).toBeNull();
    completeDelete();
    await vi.waitFor(() => {
      expect(container.querySelector("[data-history-hash]")).toBeNull();
      expect(document.activeElement).toBe(container.querySelector("main"));
      expect(onHistoryChanged).toHaveBeenCalledOnce();
    });
  });

  it("does not replay an earlier native search request when a delete dialog closes", async () => {
    await renderHistory("en", false, 1);
    vi.mocked(window.requestAnimationFrame).mock.calls[0][0](0);
    expect(document.activeElement).toBe(container.querySelector("input"));
    const trigger = deleteButton();
    flushSync(() => trigger.click());
    flushSync(() => dialogButtons()[0].click());
    expect(window.requestAnimationFrame).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(trigger);
  });

  it("shows a failed delete, and requires confirmation again before retrying", async () => {
    deleteCommand.mockRejectedValueOnce({
      code: "database_operation_failed",
      operation: "delete_history",
      retryable: true,
    });
    await renderHistory();
    flushSync(() => deleteButton().click());
    flushSync(() => dialogButtons()[1].click());
    await vi.waitFor(() => {
      expect(
        container.querySelector(".error-banner-content > p")?.textContent
      ).toBe("This clipboard item could not be deleted.");
      expect(deleteButton().disabled).toBe(false);
    });
    flushSync(() => retryButton().click());
    expect(deleteCalls()).toHaveLength(1);
    flushSync(() => dialogButtons()[0].click());
    expect(deleteCalls()).toHaveLength(1);
    flushSync(() => retryButton().click());
    flushSync(() => dialogButtons()[1].click());
    await vi.waitFor(() => {
      expect(container.querySelector("[data-history-hash]")).toBeNull();
      expect(container.querySelector(".error-banner")).toBeNull();
      expect(onHistoryChanged).toHaveBeenCalledOnce();
    });
    expect(deleteCalls()).toEqual([
      ["delete_copy_event", { contentHash: targetHash }],
      ["delete_copy_event", { contentHash: targetHash }],
    ]);
  });

  it.each([
    ["en", "Delete this pinned item?"],
    ["zh-CN", "删除此固定项目？"],
    ["zh-TW", "刪除此固定項目？"],
  ] as const)(
    "localizes the warning and compact group deletion in %s",
    async (language, title) => {
      await renderHistory(language, true);
      flushSync(() => deleteButton(language).click());
      const dialog = container.querySelector('[role="dialog"]')!;
      const localized = getMessages(language);
      expect(dialog.querySelector("h3")?.textContent).toBe(title);
      expect(dialog.textContent).toContain(
        localized.deletePinnedConfirmationDescription
      );
      expect(dialog.textContent).toContain(
        localized.deletePinnedCompactDescription
      );
      expect(dialog.textContent).toContain(localized.cannotUndo);
      flushSync(() => dialogButtons()[0].click());
      expect(deleteCalls()).toHaveLength(0);
    }
  );
});
