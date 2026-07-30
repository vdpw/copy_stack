export const appPages = ["history", "settings"] as const;

export type AppPage = (typeof appPages)[number];

export function isAppPage(value: unknown): value is AppPage {
  return appPages.some(page => page === value);
}
