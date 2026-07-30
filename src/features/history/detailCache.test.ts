import { describe, expect, it } from "vitest";
import type { HistoryDetail } from "../../types";
import { canLoadHistoryDetail, HistoryDetailCache } from "./detailCache";

function detail(contentHash: string): HistoryDetail {
  return {
    content_hash: contentHash,
    html_preview: null,
    text_preview: null,
    rich_preview: [],
  };
}

describe("HistoryDetailCache", () => {
  it("gates detail across compact-mode transitions and summary metadata", () => {
    expect(canLoadHistoryDetail(false, true)).toBe(true);
    expect(canLoadHistoryDetail(true, true)).toBe(false);
    expect(canLoadHistoryDetail(false, false)).toBe(false);
  });

  it("keeps a bounded least-recently-used cache", async () => {
    const cache = new HistoryDetailCache(
      async contentHash => detail(contentHash),
      2
    );

    await cache.load("first");
    await cache.load("second");
    await cache.load("first");
    await cache.load("third");

    expect(cache.size).toBe(2);
    expect(cache.peek("first")).toEqual(detail("first"));
    expect(cache.peek("second")).toBeUndefined();
    expect(cache.peek("third")).toEqual(detail("third"));
  });

  it("drops a response from an invalidated request generation", async () => {
    let resolveRequest: ((value: HistoryDetail) => void) | undefined;
    const cache = new HistoryDetailCache(
      contentHash =>
        new Promise(resolve => {
          resolveRequest = resolve;
          expect(contentHash).toBe("late");
        })
    );

    const pending = cache.load("late");
    cache.reset();
    resolveRequest?.(detail("late"));

    await expect(pending).resolves.toBeUndefined();
    expect(cache.peek("late")).toBeUndefined();
  });
});
