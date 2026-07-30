import { describe, expect, it } from "vitest";
import { isAppPage } from "./navigation";

describe("isAppPage", () => {
  it("accepts the two in-window destinations", () => {
    expect(isAppPage("history")).toBe(true);
    expect(isAppPage("settings")).toBe(true);
  });

  it("rejects unknown native navigation payloads", () => {
    expect(isAppPage("preferences")).toBe(false);
    expect(isAppPage(null)).toBe(false);
    expect(isAppPage({ page: "history" })).toBe(false);
  });
});
