interface RefreshAfterClipboardUpdateOptions {
  refresh: () => Promise<boolean>;
  refreshPreservingView: () => Promise<boolean>;
  resetScrollToTop: () => Promise<void>;
  windowFocused: boolean;
}

export function shouldScrollToTopAfterRestore(
  moveRestoredItemToTop: boolean,
  restoredContentHash: string,
  firstContentHash: string | undefined
): boolean {
  return (
    moveRestoredItemToTop &&
    firstContentHash !== undefined &&
    restoredContentHash !== firstContentHash
  );
}

export async function refreshHistoryToTop(
  refresh: () => Promise<boolean>,
  resetScrollToTop: () => Promise<void>
): Promise<boolean> {
  const refreshed = await refresh();
  if (refreshed) {
    await resetScrollToTop();
  }
  return refreshed;
}

export async function refreshAfterClipboardUpdate({
  refresh,
  refreshPreservingView,
  resetScrollToTop,
  windowFocused,
}: RefreshAfterClipboardUpdateOptions): Promise<boolean> {
  if (windowFocused) {
    return refreshPreservingView();
  }

  return refreshHistoryToTop(refresh, resetScrollToTop);
}
