export const historyScrollToTopDurationMs = 320;

interface ScrollAnimationDriver {
  now: () => number;
  prefersReducedMotion: () => boolean;
  requestFrame: (callback: (timestamp: number) => void) => void;
}

function browserScrollAnimationDriver(): ScrollAnimationDriver {
  return {
    now: () => window.performance.now(),
    prefersReducedMotion: () =>
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
    requestFrame: callback => {
      window.requestAnimationFrame(callback);
    },
  };
}

export function animateHistoryScrollToTop(
  container: Pick<HTMLElement, "scrollTop" | "scrollTo">,
  durationMs = historyScrollToTopDurationMs,
  driver: ScrollAnimationDriver = browserScrollAnimationDriver()
): Promise<void> {
  const startY = Math.max(0, container.scrollTop);
  if (startY === 0 || durationMs <= 0 || driver.prefersReducedMotion()) {
    container.scrollTo({ top: 0 });
    return Promise.resolve();
  }

  const startedAt = driver.now();
  return new Promise(resolve => {
    const step = (timestamp: number) => {
      const progress = Math.min(
        Math.max((timestamp - startedAt) / durationMs, 0),
        1
      );
      const easedProgress = 1 - Math.pow(1 - progress, 3);
      container.scrollTo({ top: startY * (1 - easedProgress) });
      if (progress < 1) {
        driver.requestFrame(step);
      } else {
        resolve();
      }
    };

    driver.requestFrame(step);
  });
}
