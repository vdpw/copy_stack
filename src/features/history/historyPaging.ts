import type { HistoryPage, HistorySummary } from "../../types";

export type HistoryPageLoader = (cursor: string | null) => Promise<HistoryPage>;

function mergeUnique(
  current: readonly HistorySummary[],
  incoming: readonly HistorySummary[]
): HistorySummary[] {
  const merged = new Map<string, HistorySummary>();
  for (const item of [...current, ...incoming]) {
    if (!merged.has(item.content_hash)) {
      merged.set(item.content_hash, item);
    }
  }
  return Array.from(merged.values());
}

export async function loadHistoryDepth(
  loadPage: HistoryPageLoader,
  targetCount: number,
  isCurrent: () => boolean = () => true
): Promise<HistoryPage | undefined> {
  let page = await loadPage(null);
  if (!isCurrent()) {
    return undefined;
  }

  let items = page.items;
  let cursor = page.next_cursor;
  let hasMore = page.has_more;
  while (hasMore && cursor && items.length < targetCount) {
    const nextPage = await loadPage(cursor);
    if (!isCurrent()) {
      return undefined;
    }
    items = mergeUnique(items, nextPage.items);
    cursor = nextPage.next_cursor;
    hasMore = nextPage.has_more;
    page = {
      ...nextPage,
      items,
      total_count: nextPage.total_count ?? page.total_count,
      total_bytes: nextPage.total_bytes ?? page.total_bytes,
    };
  }

  return {
    ...page,
    items,
    next_cursor: cursor,
    has_more: hasMore,
  };
}
