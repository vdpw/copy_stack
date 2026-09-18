// @vitest-environment jsdom

import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ThemePreference } from "../types";
import { useAppearance } from "./useAppearance";

const roots: ReturnType<typeof createRoot>[] = [];
afterEach(() => {
  roots.splice(0).forEach(root => flushSync(() => root.unmount()));
  delete document.documentElement.dataset.theme;
  vi.unstubAllGlobals();
});

function Appearance({ theme }: { theme: ThemePreference }) {
  useAppearance(theme);
  return null;
}

function mockSystemTheme(initialDark: boolean) {
  const listeners = new Set<() => void>();
  const media = {
    matches: initialDark,
    addEventListener: vi.fn((_event: string, listener: () => void) =>
      listeners.add(listener)
    ),
    removeEventListener: vi.fn((_event: string, listener: () => void) =>
      listeners.delete(listener)
    ),
  };
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => media)
  );
  return {
    media,
    change(dark: boolean) {
      media.matches = dark;
      listeners.forEach(listener => listener());
    },
  };
}

describe("useAppearance", () => {
  it("follows OS changes only in system mode and cleans up listeners", () => {
    const system = mockSystemTheme(true);
    const root = createRoot(document.createElement("div"));
    roots.push(root);
    const render = (theme: ThemePreference) =>
      flushSync(() => root.render(<Appearance theme={theme} />));
    render("system");
    expect(document.documentElement.dataset.theme).toBe("dark");
    system.change(false);
    expect(document.documentElement.dataset.theme).toBe("light");
    render("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(system.media.removeEventListener).toHaveBeenCalledOnce();
    system.change(false);
    expect(document.documentElement.dataset.theme).toBe("dark");
    render("light");
    system.change(true);
    expect(document.documentElement.dataset.theme).toBe("light");
    render("system");
    expect(document.documentElement.dataset.theme).toBe("dark");
    flushSync(() => root.unmount());
    roots.pop();
    expect(system.media.removeEventListener).toHaveBeenCalledTimes(2);
  });
});
