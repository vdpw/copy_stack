// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import {
  historyLoadAheadMargin,
  observeHistoryEnd,
} from "./historyInfiniteScroll";

interface TestIntersectionEntry {
  isIntersecting: boolean;
}

interface TestIntersectionOptions {
  root?: null;
  rootMargin?: string;
  threshold?: number;
}

type TestIntersectionCallback = (
  entries: TestIntersectionEntry[],
  observer: unknown
) => void;

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("observeHistoryEnd", () => {
  it("requests the next page once when the sentinel enters the load-ahead area", () => {
    let callback: TestIntersectionCallback | undefined;
    let options: TestIntersectionOptions | undefined;
    const observe = vi.fn();
    const disconnect = vi.fn();
    class IntersectionObserverMock {
      observe = observe;
      disconnect = disconnect;

      constructor(
        nextCallback: TestIntersectionCallback,
        nextOptions?: TestIntersectionOptions
      ) {
        callback = nextCallback;
        options = nextOptions;
      }
    }
    vi.stubGlobal("IntersectionObserver", IntersectionObserverMock);

    const target = document.createElement("div");
    const onVisible = vi.fn();
    const cleanup = observeHistoryEnd(target, onVisible);

    expect(observe).toHaveBeenCalledWith(target);
    expect(options).toEqual({
      root: null,
      rootMargin: historyLoadAheadMargin,
      threshold: 0,
    });

    callback?.([{ isIntersecting: false }], undefined);
    callback?.([{ isIntersecting: true }], undefined);
    callback?.([{ isIntersecting: true }], undefined);

    expect(onVisible).toHaveBeenCalledOnce();
    cleanup();
    expect(disconnect).toHaveBeenCalledOnce();
  });

  it("keeps the manual load-more fallback when observers are unavailable", () => {
    vi.stubGlobal("IntersectionObserver", undefined);
    const onVisible = vi.fn();

    expect(() =>
      observeHistoryEnd(document.createElement("div"), onVisible)()
    ).not.toThrow();
    expect(onVisible).not.toHaveBeenCalled();
  });
});
