// @vitest-environment jsdom

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import type { Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getMessages } from "../../i18n";
import type { HistorySummary } from "../../types";
import { HistoryView } from "./HistoryView";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const targetHash = "pin-target";
const summary: HistorySummary = {
  content_hash: targetHash,
  data_type: "text",
  display: [101, 120, 97, 109, 112, 108, 101],
  display_truncated: false,
  source_bundle_id: null,
  is_remote_clipboard: false,
  is_pinned: false,
  timestamp: 0,
  byte_count: 7,
  has_detail: false,
};

const messages = getMessages("zh-CN");
let container: HTMLElement;
let root: Root;
let storedItems: HistorySummary[];
const onHistoryChanged = vi.fn<() => Promise<void>>();
const pinCommand = vi.fn<(args: Record<string, unknown>) => Promise<void>>();

function applyPin(args: Record<string, unknown>) {
  storedItems = storedItems.map(item =>
    item.content_hash === args.contentHash
      ? { ...item, is_pinned: args.pinned === true }
      : item
  );
}

function targetCard(): HTMLElement {
  return container.querySelector<HTMLElement>(
    `[data-history-hash="${targetHash}"]`
  )!;
}

function targetButton(label: string): HTMLElement {
  return targetCard().querySelector<HTMLElement>(
    `button[aria-label="${label}"]`
  )!;
}

function pinCalls() {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([command]) => command === "set_copy_event_pinned");
}

function historyScroller(): HTMLElement {
  return container.querySelector<HTMLElement>("main")!;
}

async function renderHistory() {
  flushSync(() => {
    root.render(
      <HistoryView
        compactMode={false}
        focusSearchRequest={0}
        language="zh-CN"
        messages={messages}
        moveRestoredItemToTop={false}
        onHistoryChanged={onHistoryChanged}
      />
    );
  });
  const scroller = historyScroller();
  scroller.scrollTo = vi.fn(
    (options?: number | { top?: number }, top?: number) => {
      scroller.scrollTop =
        typeof options === "number" ? (top ?? 0) : (options?.top ?? 0);
    }
  );
  scroller.scrollBy = vi.fn(
    (options?: number | { top?: number }, top?: number) => {
      scroller.scrollTop +=
        typeof options === "number" ? (top ?? 0) : (options?.top ?? 0);
    }
  );
  await vi.waitFor(() => {
    expect(targetButton(messages.pinItem)).not.toBeNull();
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  storedItems = [
    { ...summary, content_hash: "recent-item", timestamp: 100 },
    { ...summary },
  ];
  onHistoryChanged.mockResolvedValue(undefined);
  pinCommand.mockReset();
  pinCommand.mockImplementation(async args => applyPin(args));
  vi.mocked(listen).mockResolvedValue(vi.fn());
  vi.spyOn(window, "requestAnimationFrame").mockReturnValue(0);
  vi.spyOn(window, "scrollTo").mockImplementation(() => undefined);
  vi.spyOn(window, "scrollBy").mockImplementation(() => undefined);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_copy_events_page") {
      const items = [...storedItems].sort(
        (left, right) => Number(right.is_pinned) - Number(left.is_pinned)
      );
      return {
        items,
        next_cursor: null,
        has_more: false,
        total_count: items.length,
        total_bytes: 14,
      };
    }
    if (command === "set_copy_event_pinned") {
      return pinCommand(args as Record<string, unknown>);
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

describe("HistoryView pin command wiring", () => {
  it("guards repeated clicks, refreshes pinned grouping, and sends the inverse unpin command", async () => {
    let completePin!: () => void;
    pinCommand.mockImplementationOnce(
      args =>
        new Promise<void>(resolve => {
          completePin = () => {
            applyPin(args);
            resolve();
          };
        })
    );
    await renderHistory();

    const pin = targetButton(messages.pinItem);
    // Both clicks happen before React commits the disabled state.
    flushSync(() => {
      pin.click();
      pin.click();
    });
    expect(pinCalls()).toEqual([
      ["set_copy_event_pinned", { contentHash: targetHash, pinned: true }],
    ]);
    expect(pin.hasAttribute("disabled")).toBe(true);
    expect(
      targetCard()
        .querySelector(".event-content")
        ?.getAttribute("aria-expanded")
    ).toBe("false");
    expect(onHistoryChanged).not.toHaveBeenCalled();

    const updateListener = vi
      .mocked(listen)
      .mock.calls.find(([event]) => event === "clipboard-history-updated")?.[1];
    expect(updateListener).toBeDefined();
    flushSync(() => {
      updateListener?.({
        event: "clipboard-history-updated",
        id: 1,
        payload: null,
      });
    });
    expect(
      vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === "get_copy_events_page")
    ).toHaveLength(1);

    completePin();
    await vi.waitFor(() => {
      const unpin = targetButton(messages.unpinItem);
      expect(unpin.getAttribute("aria-pressed")).toBe("true");
      expect(unpin.hasAttribute("disabled")).toBe(false);
      expect(onHistoryChanged).toHaveBeenCalledTimes(2);
    });
    expect(
      Array.from(container.querySelectorAll(".history-group-heading")).map(
        heading => heading.textContent
      )
    ).toEqual([messages.pinned, messages.recentHistory]);
    expect(
      container
        .querySelector("[data-history-hash]")
        ?.getAttribute("data-history-hash")
    ).toBe(targetHash);
    expect(container.querySelector('[role="status"]')?.textContent).toBe(
      messages.pinUpdated
    );
    expect(historyScroller().scrollTo).toHaveBeenCalledWith({ top: 0 });
    expect(window.scrollTo).not.toHaveBeenCalled();
    vi.mocked(historyScroller().scrollTo).mockClear();

    flushSync(() => targetButton(messages.unpinItem).click());
    await vi.waitFor(() => {
      expect(targetButton(messages.pinItem).getAttribute("aria-pressed")).toBe(
        "false"
      );
      expect(onHistoryChanged).toHaveBeenCalledTimes(3);
    });
    expect(pinCalls()[1]).toEqual([
      "set_copy_event_pinned",
      { contentHash: targetHash, pinned: false },
    ]);
    expect(container.querySelector(".event-pinned-badge")).toBeNull();
    expect(container.querySelectorAll(".history-group-heading")).toHaveLength(
      1
    );
    expect(historyScroller().scrollTo).not.toHaveBeenCalled();
    expect(window.scrollTo).not.toHaveBeenCalled();
  });

  it("keeps keyboard focus and search within the history scroll container", async () => {
    await renderHistory();
    const scroller = historyScroller();
    expect(document.activeElement).toBe(scroller);
    scroller.scrollTop = 500;
    const search = container.querySelector<HTMLElement>(
      'input[type="search"]'
    )!;
    const focus = vi.spyOn(search, "focus");

    window.dispatchEvent(
      new window.KeyboardEvent("keydown", {
        key: "f",
        metaKey: true,
        bubbles: true,
        cancelable: true,
      })
    );
    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
    expect(document.activeElement).toBe(search);
    expect(scroller.scrollTop).toBe(500);
    expect(scroller.scrollTo).not.toHaveBeenCalled();

    search.dispatchEvent(
      new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true })
    );
    expect(document.activeElement).toBe(scroller);
    expect(scroller.scrollTop).toBe(500);
    expect(window.scrollTo).not.toHaveBeenCalled();
  });

  it("restores a visible card relative to the internal viewport after a refresh", async () => {
    await renderHistory();
    const scroller = historyScroller();
    scroller.scrollTop = 500;
    const cards = container.querySelectorAll<HTMLElement>(
      "[data-history-hash]"
    );
    const viewport = vi
      .spyOn(scroller, "getBoundingClientRect")
      .mockReturnValue(new window.DOMRect(0, 80, 600, 500));
    vi.spyOn(cards[0], "getBoundingClientRect").mockReturnValue(
      new window.DOMRect(0, 20, 580, 30)
    );
    const visibleCard = vi
      .spyOn(cards[1], "getBoundingClientRect")
      .mockReturnValue(new window.DOMRect(0, 100, 580, 40));
    const navigate = vi
      .mocked(listen)
      .mock.calls.find(([event]) => event === "app:navigate")?.[1];
    expect(navigate).toBeDefined();

    flushSync(() =>
      navigate?.({ event: "app:navigate", id: 1, payload: "history" })
    );
    await vi.waitFor(() =>
      expect(window.requestAnimationFrame).toHaveBeenCalledOnce()
    );
    viewport.mockReturnValue(new window.DOMRect(0, 100, 600, 500));
    visibleCard.mockReturnValue(new window.DOMRect(0, 150, 580, 40));
    vi.mocked(window.requestAnimationFrame).mock.calls[0][0](0);

    expect(scroller.scrollBy).toHaveBeenCalledWith({ top: 30 });
    expect(scroller.scrollTop).toBe(530);
    expect(window.scrollBy).not.toHaveBeenCalled();
    expect(window.scrollTo).not.toHaveBeenCalled();
  });

  it("restores the internal scroll offset when the anchored row is removed", async () => {
    await renderHistory();
    const scroller = historyScroller();
    scroller.scrollTop = 420;
    vi.spyOn(scroller, "getBoundingClientRect").mockReturnValue(
      new window.DOMRect(0, 80, 600, 500)
    );
    vi.spyOn(targetCard(), "getBoundingClientRect").mockReturnValue(
      new window.DOMRect(0, 100, 580, 40)
    );
    const navigate = vi
      .mocked(listen)
      .mock.calls.find(([event]) => event === "app:navigate")?.[1];
    storedItems = [];
    flushSync(() =>
      navigate?.({ event: "app:navigate", id: 1, payload: "history" })
    );
    await vi.waitFor(() => {
      expect(window.requestAnimationFrame).toHaveBeenCalledOnce();
      expect(container.querySelector("[data-history-hash]")).toBeNull();
    });
    scroller.scrollTop = 0;
    vi.mocked(window.requestAnimationFrame).mock.calls[0][0](0);

    expect(scroller.scrollTo).toHaveBeenCalledWith({ top: 420 });
    expect(window.scrollTo).not.toHaveBeenCalled();
  });

  it("shows a localized pin failure and retries the same command successfully", async () => {
    pinCommand.mockRejectedValueOnce({
      code: "database_operation_failed",
      operation: "pin_history",
      retryable: true,
    });
    await renderHistory();
    flushSync(() => targetButton(messages.pinItem).click());

    await vi.waitFor(() => {
      expect(
        container.querySelector(".error-banner-content > p")?.textContent
      ).toBe("无法更新固定状态。");
      expect(targetButton(messages.pinItem).hasAttribute("disabled")).toBe(
        false
      );
    });
    expect(onHistoryChanged).not.toHaveBeenCalled();
    expect(targetCard().querySelector(".event-pinned-badge")).toBeNull();

    flushSync(() => {
      container
        .querySelector<HTMLElement>(
          `.error-banner button[aria-label="${messages.retry}"]`
        )!
        .click();
    });
    await vi.waitFor(() => {
      expect(targetButton(messages.unpinItem)).not.toBeNull();
      expect(container.querySelector(".error-banner")).toBeNull();
      expect(onHistoryChanged).toHaveBeenCalledOnce();
    });
    expect(pinCalls()).toEqual([
      ["set_copy_event_pinned", { contentHash: targetHash, pinned: true }],
      ["set_copy_event_pinned", { contentHash: targetHash, pinned: true }],
    ]);
  });
});
