import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type { ElementRef } from "react";
import {
  invokeCommand,
  normalizeCommandError,
  TauriCommandError,
} from "../../api/tauri";
import { DiagnosticErrorBanner } from "../../components/DiagnosticErrorBanner";
import type { Messages, SupportedLanguage } from "../../i18n";
import { useClipboardHistory } from "../../hooks/useClipboardHistory";
import { useHistoryDetails } from "../../hooks/useHistoryDetails";
import type { Operation } from "../../types";
import { canLoadHistoryDetail } from "./detailCache";
import { EventCard } from "./EventCard";
import { observeHistoryEnd } from "./historyInfiniteScroll";
import {
  refreshAfterClipboardUpdate,
  refreshHistoryToTop,
  shouldScrollToTopAfterRestore,
} from "./historyRefresh";
import { animateHistoryScrollToTop } from "./scrollAnimation";

interface HistoryViewProps {
  compactMode: boolean;
  language: SupportedLanguage;
  messages: Messages;
  moveRestoredItemToTop: boolean;
  onHistoryChanged: () => Promise<unknown>;
}

interface ScrollAnchor {
  contentHash: string | null;
  offset: number;
  scrollY: number;
}

interface ActionFailure {
  error: TauriCommandError;
  retry: (() => void) | null;
}

function captureScrollAnchor(container: HTMLElement | null): ScrollAnchor {
  if (!container) {
    return { contentHash: null, offset: 0, scrollY: window.scrollY };
  }

  const cards = Array.from(
    container.querySelectorAll<HTMLElement>("[data-history-hash]")
  );
  const anchor =
    cards.find(card => card.getBoundingClientRect().bottom > 0) ?? null;
  return {
    contentHash: anchor?.dataset.historyHash ?? null,
    offset: anchor?.getBoundingClientRect().top ?? 0,
    scrollY: window.scrollY,
  };
}

function restoreScrollAnchor(
  container: HTMLElement | null,
  anchor: ScrollAnchor
): void {
  window.requestAnimationFrame(() => {
    const anchoredCard = anchor.contentHash
      ? container?.querySelector<HTMLElement>(
          `[data-history-hash="${anchor.contentHash}"]`
        )
      : null;
    if (anchoredCard) {
      window.scrollBy({
        top: anchoredCard.getBoundingClientRect().top - anchor.offset,
      });
    } else {
      window.scrollTo({ top: anchor.scrollY });
    }
  });
}

export function HistoryView({
  compactMode,
  language,
  messages,
  moveRestoredItemToTop,
  onHistoryChanged,
}: HistoryViewProps) {
  const {
    dismissError: dismissHistoryError,
    error: historyError,
    hasMore,
    items: historyItems,
    loadMore,
    loading,
    loadingMore,
    refresh: refreshHistory,
    totalCount,
  } = useClipboardHistory();
  const {
    details: loadedDetails,
    errors: detailErrors,
    load: loadDetail,
    loading: loadingDetails,
    remove: removeDetail,
    reset: resetDetails,
    retain: retainDetails,
  } = useHistoryDetails();
  const listRef = useRef<ElementRef<"div"> | null>(null);
  const loadMoreSentinelRef = useRef<ElementRef<"div"> | null>(null);
  const copiedFeedbackTimerRef = useRef<number | null>(null);
  const restoringHashesRef = useRef(new Set<string>());
  const pendingRestoreToTopIntentsRef = useRef(new Set<number>());
  const restoreToTopIntentSequenceRef = useRef(0);
  const [copiedEventHash, setCopiedEventHash] = useState<string | null>(null);
  const [restoringEventHashes, setRestoringEventHashes] = useState<Set<string>>(
    () => new Set()
  );
  const [expandedEventHashes, setExpandedEventHashes] = useState<Set<string>>(
    () => new Set()
  );
  const [actionFailure, setActionFailure] = useState<ActionFailure | null>(
    null
  );
  const [captureNotice, setCaptureNotice] = useState(false);

  const refreshPreservingView = useCallback(async () => {
    const anchor = captureScrollAnchor(listRef.current);
    const refreshed = await refreshHistory();
    if (refreshed) {
      restoreScrollAnchor(listRef.current, anchor);
    }
    return refreshed;
  }, [refreshHistory]);

  const resetScrollToTop = useCallback((): Promise<void> => {
    return animateHistoryScrollToTop();
  }, []);

  const refreshForClipboardUpdate = useCallback(
    () =>
      refreshAfterClipboardUpdate({
        refresh: refreshHistory,
        refreshPreservingView,
        resetScrollToTop,
        windowFocused: document.hasFocus(),
      }),
    [refreshHistory, refreshPreservingView, resetScrollToTop]
  );

  const reportActionFailure = useCallback(
    (caught: unknown, operation: Operation, retry: (() => void) | null) => {
      setActionFailure({
        error: normalizeCommandError(caught, operation),
        retry,
      });
    },
    []
  );

  const showCopiedFeedback = useCallback((contentHash: string) => {
    setCopiedEventHash(contentHash);
    if (copiedFeedbackTimerRef.current !== null) {
      window.clearTimeout(copiedFeedbackTimerRef.current);
    }
    copiedFeedbackTimerRef.current = window.setTimeout(() => {
      setCopiedEventHash(null);
      copiedFeedbackTimerRef.current = null;
    }, 1400);
  }, []);

  const deleteEvent = useCallback(
    async (contentHash: string) => {
      try {
        await invokeCommand<void>("delete_copy_event", "delete_history", {
          contentHash,
        });
        removeDetail(contentHash);
        setExpandedEventHashes(current => {
          const next = new Set(current);
          next.delete(contentHash);
          return next;
        });
        setActionFailure(null);
        await refreshPreservingView();
        await onHistoryChanged();
      } catch (caught) {
        reportActionFailure(caught, "delete_history", () => {
          void deleteEvent(contentHash);
        });
      }
    },
    [onHistoryChanged, removeDetail, refreshPreservingView, reportActionFailure]
  );

  const restoreEvent = useCallback(
    async (contentHash: string) => {
      if (restoringHashesRef.current.has(contentHash)) {
        return;
      }
      const shouldScrollToTop = shouldScrollToTopAfterRestore(
        moveRestoredItemToTop,
        contentHash,
        historyItems[0]?.content_hash
      );
      const restoreToTopIntent = shouldScrollToTop
        ? ++restoreToTopIntentSequenceRef.current
        : null;
      if (restoreToTopIntent !== null) {
        pendingRestoreToTopIntentsRef.current.add(restoreToTopIntent);
      }
      restoringHashesRef.current.add(contentHash);
      setRestoringEventHashes(current => new Set(current).add(contentHash));
      try {
        await invokeCommand<void>("copy_to_clipboard", "restore_clipboard", {
          contentHash,
        });
        setActionFailure(null);
        showCopiedFeedback(contentHash);
        if (restoreToTopIntent !== null) {
          await refreshHistoryToTop(refreshHistory, resetScrollToTop);
        }
      } catch (caught) {
        const commandError = normalizeCommandError(caught, "restore_clipboard");
        if (commandError.code === "restore_post_processing_failed") {
          showCopiedFeedback(contentHash);
        }
        setActionFailure({
          error: commandError,
          retry: commandError.retryable
            ? () => {
                void restoreEvent(contentHash);
              }
            : null,
        });
      } finally {
        if (restoreToTopIntent !== null) {
          pendingRestoreToTopIntentsRef.current.delete(restoreToTopIntent);
        }
        restoringHashesRef.current.delete(contentHash);
        setRestoringEventHashes(current => {
          const next = new Set(current);
          next.delete(contentHash);
          return next;
        });
      }
    },
    [
      historyItems,
      moveRestoredItemToTop,
      refreshHistory,
      resetScrollToTop,
      showCopiedFeedback,
    ]
  );

  const toggleExpansion = useCallback(
    (contentHash: string, hasDetail: boolean) => {
      const expanding = !expandedEventHashes.has(contentHash);
      setExpandedEventHashes(current => {
        const next = new Set(current);
        if (next.has(contentHash)) {
          next.delete(contentHash);
        } else {
          next.add(contentHash);
        }
        return next;
      });

      if (expanding && hasDetail && !loadedDetails.has(contentHash)) {
        void loadDetail(contentHash);
      }
    },
    [expandedEventHashes, loadDetail, loadedDetails]
  );

  useEffect(() => {
    return () => {
      if (copiedFeedbackTimerRef.current !== null) {
        window.clearTimeout(copiedFeedbackTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    resetDetails();
    setExpandedEventHashes(new Set());
  }, [compactMode, resetDetails]);

  useEffect(() => {
    retainDetails(new Set(historyItems.map(summary => summary.content_hash)));
  }, [historyItems, retainDetails]);

  useEffect(() => {
    const sentinel = loadMoreSentinelRef.current;
    if (!sentinel || !hasMore || loadingMore || historyError) {
      return;
    }

    return observeHistoryEnd(sentinel, () => {
      void loadMore();
    });
  }, [hasMore, historyError, loadMore, loadingMore]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: (() => void)[] = [];

    const register = async () => {
      const historyUnlisten = await listen("clipboard-history-updated", () => {
        if (pendingRestoreToTopIntentsRef.current.size === 0) {
          void refreshForClipboardUpdate();
        }
        void onHistoryChanged();
      });
      if (disposed) {
        historyUnlisten();
        return;
      }
      unlisteners.push(historyUnlisten);

      const captureUnlisten = await listen("capture-rejected", () => {
        setCaptureNotice(true);
      });
      if (disposed) {
        captureUnlisten();
        return;
      }
      unlisteners.push(captureUnlisten);

      const navigateUnlisten = await listen<string>("app:navigate", event => {
        if (event.payload === "history") {
          void refreshPreservingView();
        }
      });
      if (disposed) {
        navigateUnlisten();
        return;
      }
      unlisteners.push(navigateUnlisten);
    };

    void register().catch(caught => {
      unlisteners.forEach(unlisten => unlisten());
      reportActionFailure(caught, "load_history", () => {
        void refreshPreservingView();
      });
    });

    return () => {
      disposed = true;
      unlisteners.forEach(unlisten => unlisten());
    };
  }, [
    onHistoryChanged,
    refreshForClipboardUpdate,
    refreshPreservingView,
    reportActionFailure,
  ]);

  const visibleFailure = actionFailure?.error ?? historyError;
  const retryVisibleFailure =
    actionFailure?.retry ??
    (() => {
      void refreshPreservingView();
    });

  return (
    <div className="workspace">
      <main className="content-panel">
        <h1 className="sr-only">{messages.clipboardHistory}</h1>

        {visibleFailure && (
          <DiagnosticErrorBanner
            error={visibleFailure}
            messages={messages}
            onDismiss={() => {
              setActionFailure(null);
              dismissHistoryError();
            }}
            onRetry={retryVisibleFailure}
          />
        )}

        {captureNotice && (
          <aside aria-live="polite" className="capture-notice" role="status">
            <span>{messages.captureRejected}</span>
            <button
              aria-label={messages.dismiss}
              className="capture-notice-dismiss"
              onClick={() => setCaptureNotice(false)}
              type="button"
            >
              {messages.dismiss}
            </button>
          </aside>
        )}

        {loading ? (
          <div className="placeholder-card">{messages.loadingHistory}</div>
        ) : historyItems.length === 0 ? (
          <div className="empty-state">
            <h3>{messages.emptyHistory}</h3>
            <p>
              {compactMode
                ? messages.emptyHistoryCompact
                : messages.emptyHistoryAll}
            </p>
          </div>
        ) : (
          <>
            <div className="events-list" ref={listRef}>
              {historyItems.map(summary => (
                <EventCard
                  copied={copiedEventHash === summary.content_hash}
                  detail={
                    canLoadHistoryDetail(compactMode, summary.has_detail)
                      ? loadedDetails.get(summary.content_hash)
                      : undefined
                  }
                  detailFailed={
                    canLoadHistoryDetail(compactMode, summary.has_detail) &&
                    detailErrors.has(summary.content_hash)
                  }
                  detailLoading={
                    canLoadHistoryDetail(compactMode, summary.has_detail) &&
                    loadingDetails.has(summary.content_hash)
                  }
                  expanded={expandedEventHashes.has(summary.content_hash)}
                  key={summary.content_hash}
                  language={language}
                  messages={messages}
                  onDelete={() => void deleteEvent(summary.content_hash)}
                  onRestore={() => void restoreEvent(summary.content_hash)}
                  onRetryDetail={() => void loadDetail(summary.content_hash)}
                  onToggle={() =>
                    toggleExpansion(
                      summary.content_hash,
                      canLoadHistoryDetail(compactMode, summary.has_detail)
                    )
                  }
                  restoring={restoringEventHashes.has(summary.content_hash)}
                  summary={summary}
                />
              ))}
            </div>

            <div className="history-pagination">
              <p aria-live="polite">
                {messages.loadedHistoryCount(historyItems.length, totalCount)}
              </p>
              {hasMore && (
                <div
                  aria-live="polite"
                  className="history-footer-actions history-load-more-sentinel"
                  ref={loadMoreSentinelRef}
                >
                  <button
                    className="btn btn-secondary"
                    disabled={loadingMore}
                    onClick={() => void loadMore()}
                    type="button"
                  >
                    {loadingMore ? messages.loadingMore : messages.loadMore}
                  </button>
                </div>
              )}
            </div>
          </>
        )}

        <span aria-live="polite" className="sr-only">
          {copiedEventHash ? messages.clipboardItemCopied : ""}
        </span>
      </main>
    </div>
  );
}
