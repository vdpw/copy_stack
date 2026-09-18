import { useLayoutEffect } from "react";
import type { ThemePreference } from "../types";

/** Resolve the saved preference for every app page and native form control. */
export function useAppearance(preference: ThemePreference): void {
  useLayoutEffect(() => {
    const root = document.documentElement;
    const focused = () => {
      root.dataset.windowFocused = "true";
    };
    const blurred = () => {
      root.dataset.windowFocused = "false";
    };
    root.dataset.windowFocused = String(document.hasFocus());
    window.addEventListener("focus", focused);
    window.addEventListener("blur", blurred);
    return () => {
      window.removeEventListener("focus", focused);
      window.removeEventListener("blur", blurred);
      delete root.dataset.windowFocused;
    };
  }, []);

  useLayoutEffect(() => {
    const systemTheme = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme =
        preference === "system"
          ? systemTheme.matches
            ? "dark"
            : "light"
          : preference;
    };
    apply();
    if (preference !== "system") return;
    systemTheme.addEventListener("change", apply);
    return () => systemTheme.removeEventListener("change", apply);
  }, [preference]);
}
