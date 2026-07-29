import { useCallback, useRef, useState } from "react";
import { invokeCommand } from "../api/tauri";
import type { TauriCommandError } from "../api/tauri";
import {
  HISTORY_DETAIL_CACHE_CAPACITY,
  HistoryDetailCache,
} from "../features/history/detailCache";
import type { HistoryDetail } from "../types";

interface HistoryDetailState {
  details: ReadonlyMap<string, HistoryDetail>;
  loading: ReadonlySet<string>;
  errors: ReadonlyMap<string, TauriCommandError>;
}

const initialState: HistoryDetailState = {
  details: new Map(),
  loading: new Set(),
  errors: new Map(),
};

function setBoundedDetail(
  details: ReadonlyMap<string, HistoryDetail>,
  contentHash: string,
  detail: HistoryDetail
): Map<string, HistoryDetail> {
  const next = new Map(details);
  next.delete(contentHash);
  next.set(contentHash, detail);
  while (next.size > HISTORY_DETAIL_CACHE_CAPACITY) {
    const oldest = next.keys().next().value;
    if (typeof oldest !== "string") {
      break;
    }
    next.delete(oldest);
  }
  return next;
}

function trimSet(values: Set<string>): Set<string> {
  while (values.size > HISTORY_DETAIL_CACHE_CAPACITY) {
    const oldest = values.values().next().value;
    if (typeof oldest !== "string") {
      break;
    }
    values.delete(oldest);
  }
  return values;
}

function trimErrorMap(
  errors: Map<string, TauriCommandError>
): Map<string, TauriCommandError> {
  while (errors.size > HISTORY_DETAIL_CACHE_CAPACITY) {
    const oldest = errors.keys().next().value;
    if (typeof oldest !== "string") {
      break;
    }
    errors.delete(oldest);
  }
  return errors;
}

export function useHistoryDetails() {
  const [state, setState] = useState<HistoryDetailState>(initialState);
  const cacheRef = useRef(
    new HistoryDetailCache(contentHash =>
      invokeCommand<HistoryDetail>(
        "get_history_detail",
        "load_history_detail",
        { contentHash }
      )
    )
  );

  const load = useCallback(async (contentHash: string) => {
    const cached = cacheRef.current.peek(contentHash);
    if (cached) {
      setState(current => {
        const details = setBoundedDetail(current.details, contentHash, cached);
        return { ...current, details };
      });
      return cached;
    }

    setState(current => {
      const loading = new Set(current.loading);
      const errors = new Map(current.errors);
      loading.add(contentHash);
      errors.delete(contentHash);
      return { ...current, loading: trimSet(loading), errors };
    });

    try {
      const detail = await cacheRef.current.load(contentHash);
      setState(current => {
        const loading = new Set(current.loading);
        let details = new Map(current.details);
        loading.delete(contentHash);
        if (detail) {
          details = setBoundedDetail(details, contentHash, detail);
        }
        return { ...current, loading, details };
      });
      return detail;
    } catch (error) {
      const commandError = error as TauriCommandError;
      setState(current => {
        const loading = new Set(current.loading);
        const errors = new Map(current.errors);
        loading.delete(contentHash);
        errors.set(contentHash, commandError);
        return {
          ...current,
          loading,
          errors: trimErrorMap(errors),
        };
      });
      return undefined;
    }
  }, []);

  const remove = useCallback((contentHash: string) => {
    cacheRef.current.remove(contentHash);
    setState(current => {
      const details = new Map(current.details);
      const loading = new Set(current.loading);
      const errors = new Map(current.errors);
      details.delete(contentHash);
      loading.delete(contentHash);
      errors.delete(contentHash);
      return { details, loading, errors };
    });
  }, []);

  const retain = useCallback((contentHashes: ReadonlySet<string>) => {
    cacheRef.current.retain(contentHashes);
    setState(current => {
      const details = new Map(
        Array.from(current.details).filter(([contentHash]) =>
          contentHashes.has(contentHash)
        )
      );
      const loading = new Set(
        Array.from(current.loading).filter(contentHash =>
          contentHashes.has(contentHash)
        )
      );
      const errors = new Map(
        Array.from(current.errors).filter(([contentHash]) =>
          contentHashes.has(contentHash)
        )
      );
      return { details, loading, errors };
    });
  }, []);

  const reset = useCallback(() => {
    cacheRef.current.reset();
    setState(initialState);
  }, []);

  return {
    ...state,
    load,
    remove,
    retain,
    reset,
  };
}
