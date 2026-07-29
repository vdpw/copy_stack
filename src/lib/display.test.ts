import { describe, expect, it } from "vitest";
import { formatBytes, parseFileDisplay, truncateContent } from "./display";

describe("display helpers", () => {
  it("truncates mixed-width text without splitting characters", () => {
    expect(truncateContent("alpha  世界  omega", 12)).toBe("alpha 世...");
    expect(truncateContent("short", 12)).toBe("short");
  });

  it("accepts only the bounded file display contract", () => {
    expect(
      parseFileDisplay(
        JSON.stringify({
          format: "copy_stack.file-items.v1",
          items: [{ type: "file", name: "report.pdf" }],
        })
      )
    ).toEqual([{ type: "file", name: "report.pdf" }]);
    expect(parseFileDisplay('{"format":"other","items":[]}')).toBeNull();
    expect(parseFileDisplay("not json")).toBeNull();
  });

  it("formats byte totals without exposing implementation details", () => {
    expect(formatBytes(0, "en")).toBe("0 B");
    expect(formatBytes(1024, "en")).toBe("1 KB");
    expect(formatBytes(2.5 * 1024 * 1024, "en")).toBe("2.5 MB");
  });
});
