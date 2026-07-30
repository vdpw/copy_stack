import { describe, expect, it, vi } from "vitest";
import { refreshAfterClipboardUpdate } from "./historyRefresh";

describe("refreshAfterClipboardUpdate", () => {
  it("preserves the visible anchor while the window is focused", async () => {
    const refresh = vi.fn(async () => true);
    const refreshPreservingView = vi.fn(async () => true);
    const resetScrollToTop = vi.fn();

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
    const resetScrollToTop = vi.fn();

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
    const resetScrollToTop = vi.fn();

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
});
