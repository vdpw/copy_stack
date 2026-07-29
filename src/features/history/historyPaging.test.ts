import { describe, expect, it, vi } from "vitest";
import type { HistoryPage, HistorySummary } from "../../types";
import { loadHistoryDepth } from "./historyPaging";

function summary(index: number): HistorySummary {
  return {
    content_hash: String(index).padStart(64, "0"),
    data_type: "text",
    display: [index % 255],
    display_truncated: false,
    source_bundle_id: null,
    is_remote_clipboard: false,
    timestamp: 10_000 - index,
    byte_count: 1,
    has_detail: true,
  };
}

function page(
  start: number,
  count: number,
  nextCursor: string | null
): HistoryPage {
  return {
    items: Array.from({ length: count }, (_, index) => summary(start + index)),
    next_cursor: nextCursor,
    has_more: nextCursor !== null,
    total_count: 120,
    total_bytes: 120,
  };
}

describe("loadHistoryDepth", () => {
  it("reloads enough cursor pages to preserve depth beyond 50 items", async () => {
    const loadPage = vi.fn(async (cursor: string | null) => {
      if (cursor === null) {
        return page(0, 50, "page-2");
      }
      if (cursor === "page-2") {
        return page(50, 50, "page-3");
      }
      return page(100, 20, null);
    });

    const result = await loadHistoryDepth(loadPage, 100);
    expect(result?.items).toHaveLength(100);
    expect(result?.next_cursor).toBe("page-3");
    expect(loadPage.mock.calls).toEqual([[null], ["page-2"]]);
  });

  it("drops results when the request generation becomes stale", async () => {
    let current = true;
    const result = await loadHistoryDepth(
      async () => {
        current = false;
        return page(0, 50, null);
      },
      50,
      () => current
    );
    expect(result).toBeUndefined();
  });
});
