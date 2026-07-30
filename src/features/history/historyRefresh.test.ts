import { describe, expect, it, vi } from "vitest";
import {
  refreshAfterClipboardUpdate,
  refreshHistoryToTop,
  shouldScrollToTopAfterRestore,
} from "./historyRefresh";

describe("refreshAfterClipboardUpdate", () => {
  it("preserves the visible anchor while the window is focused", async () => {
    const refresh = vi.fn(async () => true);
    const refreshPreservingView = vi.fn(async () => true);
    const resetScrollToTop = vi.fn(async () => undefined);

    await expect(
      refreshAfterClipboardUpdate({
        refresh,
        refreshPreservingView,
        resetScrollToTop,
        windowFocused: true,
      })
    ).resolves.toBe(true);

    expect(refreshPreservingView).toHaveBeenCalledOnce();
    expect(refresh).not.toHaveBeenCalled();
    expect(resetScrollToTop).not.toHaveBeenCalled();
  });

  it("resets to the newest item after an unfocused-window update", async () => {
    const refresh = vi.fn(async () => true);
    const refreshPreservingView = vi.fn(async () => true);
    const resetScrollToTop = vi.fn(async () => undefined);

    await expect(
      refreshAfterClipboardUpdate({
        refresh,
        refreshPreservingView,
        resetScrollToTop,
        windowFocused: false,
      })
    ).resolves.toBe(true);

    expect(refresh).toHaveBeenCalledOnce();
    expect(refreshPreservingView).not.toHaveBeenCalled();
    expect(resetScrollToTop).toHaveBeenCalledOnce();
  });

  it("does not move the list when the unfocused refresh fails", async () => {
    const resetScrollToTop = vi.fn(async () => undefined);

    await expect(
      refreshAfterClipboardUpdate({
        refresh: vi.fn(async () => false),
        refreshPreservingView: vi.fn(async () => true),
        resetScrollToTop,
        windowFocused: false,
      })
    ).resolves.toBe(false);

    expect(resetScrollToTop).not.toHaveBeenCalled();
  });

  it("waits for a successful refresh before resetting to the top", async () => {
    const order: string[] = [];

    await expect(
      refreshHistoryToTop(
        async () => {
          order.push("refresh");
          return true;
        },
        async () => {
          order.push("scroll");
        }
      )
    ).resolves.toBe(true);

    expect(order).toEqual(["refresh", "scroll"]);
  });

  it("does not reset to the top when refresh fails", async () => {
    const resetScrollToTop = vi.fn(async () => undefined);

    await expect(
      refreshHistoryToTop(async () => false, resetScrollToTop)
    ).resolves.toBe(false);
    expect(resetScrollToTop).not.toHaveBeenCalled();
  });
});

describe("shouldScrollToTopAfterRestore", () => {
  it("only selects a non-first item while restore-to-top is enabled", () => {
    expect(shouldScrollToTopAfterRestore(true, "second", "first")).toBe(true);
    expect(shouldScrollToTopAfterRestore(true, "first", "first")).toBe(false);
    expect(shouldScrollToTopAfterRestore(false, "second", "first")).toBe(false);
    expect(shouldScrollToTopAfterRestore(true, "missing", undefined)).toBe(
      false
    );
  });
});
