interface RefreshAfterClipboardUpdateOptions {
  refresh: () => Promise<boolean>;
  refreshPreservingView: () => Promise<boolean>;
  resetScrollToTop: () => void;
  windowFocused: boolean;
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

  const refreshed = await refresh();
  if (refreshed) {
    resetScrollToTop();
  }
  return refreshed;
}
