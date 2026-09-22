import { listen } from "@tauri-apps/api/event";
import { Search as SearchIcon, X } from "lucide-react";
import { Fragment, useCallback, useEffect, useRef, useState } from "react";
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
import { PinnedDeleteDialog } from "./PinnedDeleteDialog";

interface HistoryViewProps {
  compactMode: boolean;
  focusSearchRequest: number;
  language: SupportedLanguage;
  messages: Messages;
  moveRestoredItemToTop: boolean;
  onHistoryChanged: () => Promise<unknown>;
}

interface ScrollAnchor {
  contentHash: string | null;
  offset: number;
  scrollTop: number;
}

interface ActionFailure {
  error: TauriCommandError;
  retry: (() => void) | null;
}

function captureScrollAnchor(
  scrollContainer: HTMLElement | null
): ScrollAnchor {
  if (!scrollContainer) {
    return { contentHash: null, offset: 0, scrollTop: 0 };
  }

  const viewportTop = scrollContainer.getBoundingClientRect().top;
  const cards = Array.from(
    scrollContainer.querySelectorAll<HTMLElement>("[data-history-hash]")
  );
  const anchor =
    cards.find(card => card.getBoundingClientRect().bottom > viewportTop) ??
    null;
  return {
    contentHash: anchor?.dataset.historyHash ?? null,
    offset: anchor ? anchor.getBoundingClientRect().top - viewportTop : 0,
    scrollTop: scrollContainer.scrollTop,
  };
}

function restoreScrollAnchor(
  scrollContainer: HTMLElement | null,
  anchor: ScrollAnchor
): void {
  if (!scrollContainer) {
    return;
  }
  window.requestAnimationFrame(() => {
    const anchoredCard = anchor.contentHash
      ? scrollContainer.querySelector<HTMLElement>(
          `[data-history-hash="${anchor.contentHash}"]`
        )
      : null;
    if (anchoredCard) {
      scrollContainer.scrollBy({
        top:
          anchoredCard.getBoundingClientRect().top -
          scrollContainer.getBoundingClientRect().top -
          anchor.offset,
      });
    } else {
      scrollContainer.scrollTo({ top: anchor.scrollTop });
    }
  });
}

export function HistoryView({
  compactMode,
  focusSearchRequest,
  language,
  messages,
  moveRestoredItemToTop,
  onHistoryChanged,
}: HistoryViewProps) {
  const [searchInput, setSearchInput] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
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
  } = useClipboardHistory(searchQuery);
  const {
    details: loadedDetails,
    errors: detailErrors,
    load: loadDetail,
    loading: loadingDetails,
    remove: removeDetail,
    reset: resetDetails,
    retain: retainDetails,
  } = useHistoryDetails();
  const scrollContainerRef = useRef<HTMLElement | null>(null);
  const loadMoreSentinelRef = useRef<ElementRef<"div"> | null>(null);
  const searchInputRef = useRef<ElementRef<"input"> | null>(null);
  const handledFocusSearchRequestRef = useRef(0);
  const returnDeleteFocusRef = useRef<HTMLElement | null>(null);
  const deletingHashesRef = useRef(new Set<string>());
  const [deletingHashes, setDeletingHashes] = useState<Set<string>>(
    () => new Set()
  );
  const [pendingPinnedDeleteHash, setPendingPinnedDeleteHash] = useState<
    string | null
  >(null);
  const deleteDialogOpen = pendingPinnedDeleteHash !== null;
  const historyItemsRef = useRef(historyItems);
  historyItemsRef.current = historyItems;
  const copiedFeedbackTimerRef = useRef<number | null>(null);
  const pinningHashesRef = useRef(new Set<string>());
  const [pinningHashes, setPinningHashes] = useState<Set<string>>(
    () => new Set()
  );
  const [pinNotice, setPinNotice] = useState(false);
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

  useEffect(() => {
    const timeout = window.setTimeout(() => {
      setSearchQuery(searchInput.trim());
    }, 180);
    return () => window.clearTimeout(timeout);
  }, [searchInput]);

  useEffect(() => {
    scrollContainerRef.current?.focus({ preventScroll: true });
  }, []);

  useEffect(() => {
    if (
      focusSearchRequest > 0 &&
      focusSearchRequest !== handledFocusSearchRequestRef.current &&
      !deleteDialogOpen
    ) {
      const frame = window.requestAnimationFrame(() => {
        handledFocusSearchRequestRef.current = focusSearchRequest;
        searchInputRef.current?.focus({ preventScroll: true });
        searchInputRef.current?.select();
      });
      return () => window.cancelAnimationFrame(frame);
    }
  }, [deleteDialogOpen, focusSearchRequest]);

  useEffect(() => {
    const focusSearch = (event: globalThis.KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        if (deleteDialogOpen) return;
        searchInputRef.current?.focus({ preventScroll: true });
        searchInputRef.current?.select();
      }
    };
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, [deleteDialogOpen]);

  useEffect(() => {
    if (!deleteDialogOpen) return;
    const background = scrollContainerRef.current;
    background?.setAttribute("inert", "");
    return () => background?.removeAttribute("inert");
  }, [deleteDialogOpen]);

  useEffect(() => {
    if (deleteDialogOpen || deletingHashes.size > 0) return;
    const trigger = returnDeleteFocusRef.current;
    if (!trigger) return;
    returnDeleteFocusRef.current = null;
    const target =
      trigger.isConnected && !trigger.matches(":disabled")
        ? trigger
        : scrollContainerRef.current;
    target?.focus({ preventScroll: true });
  }, [deleteDialogOpen, deletingHashes]);

  const refreshPreservingView = useCallback(async () => {
    const anchor = captureScrollAnchor(scrollContainerRef.current);
    const refreshed = await refreshHistory();
    if (refreshed) {
      restoreScrollAnchor(scrollContainerRef.current, anchor);
    }
    return refreshed;
  }, [refreshHistory]);

  const resetScrollToTop = useCallback((): Promise<void> => {
    return scrollContainerRef.current
      ? animateHistoryScrollToTop(scrollContainerRef.current)
      : Promise.resolve();
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

  const setPinned = useCallback(
    async (contentHash: string, pinned: boolean) => {
      if (pinningHashesRef.current.has(contentHash)) return;
      pinningHashesRef.current.add(contentHash);
      setPinningHashes(new Set(pinningHashesRef.current));
      setPinNotice(false);
      try {
        await invokeCommand<void>("set_copy_event_pinned", "pin_history", {
          contentHash,
          pinned,
        });
        setActionFailure(null);
        if (pinned) {
          await refreshHistoryToTop(refreshHistory, resetScrollToTop);
        } else {
          await refreshPreservingView();
        }
        await onHistoryChanged();
        setPinNotice(true);
      } catch (caught) {
        reportActionFailure(caught, "pin_history", () => {
          void setPinned(contentHash, pinned);
        });
      } finally {
        pinningHashesRef.current.delete(contentHash);
        setPinningHashes(new Set(pinningHashesRef.current));
      }
    },
    [
      onHistoryChanged,
      refreshHistory,
      refreshPreservingView,
      reportActionFailure,
      resetScrollToTop,
    ]
  );

  const confirmPinnedDelete = useCallback(
    (contentHash: string, trigger: HTMLElement | null) => {
      returnDeleteFocusRef.current = trigger;
      setPendingPinnedDeleteHash(contentHash);
    },
    []
  );

  const deleteEvent = useCallback(
    async (contentHash: string, wasPinned = false) => {
      if (deletingHashesRef.current.has(contentHash)) return;
      deletingHashesRef.current.add(contentHash);
      setDeletingHashes(new Set(deletingHashesRef.current));
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
          const isPinned = historyItemsRef.current.some(
            item => item.content_hash === contentHash && item.is_pinned
          );
          if (wasPinned || isPinned) {
            confirmPinnedDelete(
              contentHash,
              document.activeElement instanceof HTMLElement
                ? document.activeElement
                : null
            );
          } else {
            void deleteEvent(contentHash);
          }
        });
      } finally {
        deletingHashesRef.current.delete(contentHash);
        setDeletingHashes(new Set(deletingHashesRef.current));
      }
    },
    [
      confirmPinnedDelete,
      onHistoryChanged,
      removeDetail,
      refreshPreservingView,
      reportActionFailure,
    ]
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
  }, [compactMode, resetDetails, searchQuery]);

  useEffect(() => {
    retainDetails(new Set(historyItems.map(summary => summary.content_hash)));
  }, [historyItems, retainDetails]);

  useEffect(() => {
    const sentinel = loadMoreSentinelRef.current;
    const scrollContainer = scrollContainerRef.current;
    if (
      !sentinel ||
      !scrollContainer ||
      !hasMore ||
      loadingMore ||
      historyError
    ) {
      return;
    }

    return observeHistoryEnd(
      sentinel,
      () => {
        void loadMore();
      },
      scrollContainer
    );
  }, [hasMore, historyError, loadMore, loadingMore]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: (() => void)[] = [];

    const register = async () => {
      const historyUnlisten = await listen("clipboard-history-updated", () => {
        if (
          pendingRestoreToTopIntentsRef.current.size === 0 &&
          pinningHashesRef.current.size === 0
        ) {
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
      <main
        aria-labelledby="history-title"
        className="content-panel"
        ref={scrollContainerRef}
        tabIndex={-1}
      >
        <header className="history-header">
          <h1 id="history-title">{messages.clipboardHistory}</h1>
          <span className="history-count">{totalCount}</span>
        </header>

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

        <form
          className="history-search"
          onSubmit={event => event.preventDefault()}
          role="search"
        >
          <SearchIcon aria-hidden="true" size={18} strokeWidth={2} />
          <input
            aria-label={messages.searchClipboardHistory}
            maxLength={256}
            onChange={event => setSearchInput(event.target.value)}
            onKeyDown={event => {
              if (event.key === "Escape") {
                if (searchInput) {
                  setSearchInput("");
                  setSearchQuery("");
                } else {
                  scrollContainerRef.current?.focus({ preventScroll: true });
                }
              }
            }}
            placeholder={messages.searchClipboardHistory}
            ref={searchInputRef}
            spellCheck={false}
            type="search"
            value={searchInput}
          />
          {searchInput && (
            <button
              aria-label={messages.clearSearch}
              className="history-search-clear"
              onClick={() => {
                setSearchInput("");
                setSearchQuery("");
                searchInputRef.current?.focus({ preventScroll: true });
              }}
              type="button"
            >
              <X aria-hidden="true" size={16} strokeWidth={2.2} />
            </button>
          )}
          <span className="history-search-shortcut" aria-hidden="true">
            ⌘F
          </span>
        </form>

        {loading ? (
          <div className="placeholder-card">{messages.loadingHistory}</div>
        ) : historyItems.length === 0 ? (
          <div className="empty-state">
            {searchQuery ? (
              <>
                <h3>{messages.noSearchResults}</h3>
                <p>{messages.noSearchResultsDescription(searchQuery)}</p>
              </>
            ) : (
              <>
                <h3>{messages.emptyHistory}</h3>
                <p>
                  {compactMode
                    ? messages.emptyHistoryCompact
                    : messages.emptyHistoryAll}
                </p>
              </>
            )}
          </div>
        ) : (
          <>
            <span className="sr-only" role="status">
              {pinNotice ? messages.pinUpdated : ""}
            </span>
            <div className="events-list">
              {historyItems.map((summary, index) => (
                <Fragment key={summary.content_hash}>
                  {(index === 0 ||
                    historyItems[index - 1].is_pinned !==
                      summary.is_pinned) && (
                    <h2 className="history-group-heading">
                      {summary.is_pinned
                        ? messages.pinned
                        : messages.recentHistory}
                    </h2>
                  )}
                  <EventCard
                    copied={copiedEventHash === summary.content_hash}
                    deleting={deletingHashes.has(summary.content_hash)}
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
                    language={language}
                    messages={messages}
                    onDelete={trigger => {
                      if (
                        deletingHashesRef.current.has(summary.content_hash) ||
                        pinningHashesRef.current.has(summary.content_hash)
                      )
                        return;
                      if (summary.is_pinned) {
                        confirmPinnedDelete(summary.content_hash, trigger);
                      } else {
                        void deleteEvent(summary.content_hash);
                      }
                    }}
                    onPin={() =>
                      void setPinned(summary.content_hash, !summary.is_pinned)
                    }
                    pinning={pinningHashes.has(summary.content_hash)}
                    onRestore={() => void restoreEvent(summary.content_hash)}
                    onRetryDetail={() => void loadDetail(summary.content_hash)}
                    onToggle={() =>
                      toggleExpansion(
                        summary.content_hash,
                        canLoadHistoryDetail(compactMode, summary.has_detail)
                      )
                    }
                    restoring={restoringEventHashes.has(summary.content_hash)}
                    searchQuery={searchQuery}
                    summary={summary}
                  />
                </Fragment>
              ))}
            </div>

            <div className="history-pagination">
              <p aria-live="polite">
                {searchQuery
                  ? messages.loadedSearchCount(historyItems.length, totalCount)
                  : messages.loadedHistoryCount(
                      historyItems.length,
                      totalCount
                    )}
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
      {pendingPinnedDeleteHash !== null && (
        <PinnedDeleteDialog
          compactMode={compactMode}
          messages={messages}
          onCancel={() => setPendingPinnedDeleteHash(null)}
          onConfirm={() => {
            setPendingPinnedDeleteHash(null);
            void deleteEvent(pendingPinnedDeleteHash, true);
          }}
        />
      )}
    </div>
  );
}
