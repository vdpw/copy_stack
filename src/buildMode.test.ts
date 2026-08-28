import { describe, expect, it } from "vitest";
import { errorPresentationPolicy } from "./buildMode";

describe("errorPresentationPolicy", () => {
  it("keeps runtime diagnostics available in development", () => {
    expect(errorPresentationPolicy(true)).toEqual({
      listenForRuntimeOperationErrors: true,
      showSafeDiagnosticDetails: true,
    });
  });

  it("keeps runtime diagnostic UI out of release builds", () => {
    expect(errorPresentationPolicy(false)).toEqual({
      listenForRuntimeOperationErrors: false,
      showSafeDiagnosticDetails: false,
    });
  });
});
