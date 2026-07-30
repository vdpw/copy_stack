export const historyLoadAheadMargin = "320px 0px";

export function observeHistoryEnd(
  target: HTMLElement,
  onVisible: () => void
): () => void {
  const Observer = globalThis.IntersectionObserver;
  if (typeof Observer !== "function") {
    return () => undefined;
  }

  let requested = false;
  const observer = new Observer(
    entries => {
      if (requested || !entries.some(entry => entry.isIntersecting)) {
        return;
      }
      requested = true;
      onVisible();
    },
    {
      root: null,
      rootMargin: historyLoadAheadMargin,
      threshold: 0,
    }
  );
  observer.observe(target);

  return () => observer.disconnect();
}
