export const historyLoadAheadMargin = "320px 0px";

export function observeHistoryEnd(
  target: HTMLElement,
  onVisible: () => void,
  root: HTMLElement
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
      root,
      rootMargin: historyLoadAheadMargin,
      threshold: 0,
    }
  );
  observer.observe(target);

  return () => observer.disconnect();
}
