import { describe, expect, it, vi } from "vitest";
import {
  animateHistoryScrollToTop,
  historyScrollToTopDurationMs,
} from "./scrollAnimation";

describe("animateHistoryScrollToTop", () => {
  it("uses the balanced 320ms duration and finishes at the top", async () => {
    let timestamp = 0;
    const frames: Array<(timestamp: number) => void> = [];
    const scrollTo = vi.fn();
    const animation = animateHistoryScrollToTop(
      { scrollTop: 800, scrollTo },
      historyScrollToTopDurationMs,
      {
        now: () => timestamp,
        prefersReducedMotion: () => false,
        requestFrame: callback => frames.push(callback),
      }
    );

    while (frames.length > 0) {
      const frame = frames.shift();
      timestamp += 80;
      frame?.(timestamp);
    }
    await animation;

    expect(historyScrollToTopDurationMs).toBe(320);
    expect(scrollTo).toHaveBeenCalledTimes(4);
    expect(scrollTo).toHaveBeenLastCalledWith({ top: 0 });
  });

  it("jumps immediately when reduced motion is enabled", async () => {
    const scrollTo = vi.fn();
    const requestFrame = vi.fn();

    await animateHistoryScrollToTop(
      { scrollTop: 800, scrollTo },
      historyScrollToTopDurationMs,
      {
        now: () => 0,
        prefersReducedMotion: () => true,
        requestFrame,
      }
    );

    expect(scrollTo).toHaveBeenCalledOnce();
    expect(scrollTo).toHaveBeenCalledWith({ top: 0 });
    expect(requestFrame).not.toHaveBeenCalled();
  });
});
