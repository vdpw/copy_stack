import { useCallback, useEffect, useRef, useState } from "react";
import {
  invokeCommand,
  normalizeCommandError,
  TauriCommandError,
} from "../api/tauri";
import { loadHistoryDepth } from "../features/history/historyPaging";
import type { HistoryPage, HistorySummary, Operation } from "../types";

const historyPageSize = 50;

interface ClipboardHistoryState {
  items: HistorySummary[];
  nextCursor: string | null;
  hasMore: boolean;
  totalCount: number;
  totalBytes: number;
  loading: boolean;
  refreshing: boolean;
  loadingMore: boolean;
  error: TauriCommandError | null;
}

const initialState: ClipboardHistoryState = {
  items: [],
  nextCursor: null,
  hasMore: false,
  totalCount: 0,
  totalBytes: 0,
  loading: true,
  refreshing: false,
  loadingMore: false,
  error: null,
};

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

function runtimeTotal(
  page: HistoryPage,
  key: "total_count" | "total_bytes",
  fallback: number
): number {
  const value = page[key];
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}

export function useClipboardHistory() {
  const [state, setState] = useState<ClipboardHistoryState>(initialState);
  const generationRef = useRef(0);
  const loadMoreInFlightRef = useRef(false);
  const loadedCountRef = useRef(0);

  const refresh = useCallback(async (): Promise<boolean> => {
    const generation = ++generationRef.current;
    setState(current => ({
      ...current,
      refreshing: !current.loading,
      error: null,
    }));

    try {
      const targetCount = Math.max(historyPageSize, loadedCountRef.current);
      const page = await loadHistoryDepth(
        cursor =>
          invokeCommand<HistoryPage>("get_copy_events_page", "load_history", {
            cursor,
            pageSize: historyPageSize,
          }),
        targetCount,
        () => generation === generationRef.current
      );
      if (!page || generation !== generationRef.current) {
        return false;
      }

      loadedCountRef.current = page.items.length;
      setState({
        items: page.items,
        nextCursor: page.next_cursor,
        hasMore: page.has_more,
        totalCount: runtimeTotal(page, "total_count", page.items.length),
        totalBytes: runtimeTotal(
          page,
          "total_bytes",
          page.items.reduce((total, item) => total + item.byte_count, 0)
        ),
        loading: false,
        refreshing: false,
        loadingMore: false,
        error: null,
      });
      return true;
    } catch (error) {
      if (generation !== generationRef.current) {
        return false;
      }
      setState(current => ({
        ...current,
        loading: false,
        refreshing: false,
        loadingMore: false,
        error: normalizeCommandError(error, "load_history"),
      }));
      return false;
    }
  }, []);

  const loadMore = useCallback(async (): Promise<boolean> => {
    if (loadMoreInFlightRef.current || !state.hasMore || !state.nextCursor) {
      return false;
    }

    loadMoreInFlightRef.current = true;
    const generation = generationRef.current;
    const cursor = state.nextCursor;
    setState(current => ({ ...current, loadingMore: true, error: null }));

    try {
      const page = await invokeCommand<HistoryPage>(
        "get_copy_events_page",
        "load_history",
        { cursor, pageSize: historyPageSize }
      );
      if (generation !== generationRef.current) {
        return false;
      }

      setState(current => {
        const items = mergeUnique(current.items, page.items);
        loadedCountRef.current = items.length;
        return {
          ...current,
          items,
          nextCursor: page.next_cursor,
          hasMore: page.has_more,
          totalCount: runtimeTotal(page, "total_count", current.totalCount),
          totalBytes: runtimeTotal(page, "total_bytes", current.totalBytes),
          loadingMore: false,
          error: null,
        };
      });
      return true;
    } catch (error) {
      if (generation === generationRef.current) {
        setState(current => ({
          ...current,
          loadingMore: false,
          error: normalizeCommandError(error, "load_history"),
        }));
      }
      return false;
    } finally {
      loadMoreInFlightRef.current = false;
    }
  }, [state.hasMore, state.nextCursor]);

  const reportError = useCallback(
    (error: unknown, operation: Operation = "load_history") => {
      setState(current => ({
        ...current,
        error: normalizeCommandError(error, operation),
      }));
    },
    []
  );

  const dismissError = useCallback(() => {
    setState(current => ({ ...current, error: null }));
  }, []);

  useEffect(() => {
    void refresh();
    return () => {
      generationRef.current += 1;
    };
  }, [refresh]);

  return {
    ...state,
    refresh,
    loadMore,
    reportError,
    dismissError,
  };
}
