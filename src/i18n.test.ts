import { describe, expect, it } from "vitest";
import { getMessages, supportedLanguages } from "./i18n";

describe("metadata and error localization", () => {
  it("localizes explicit unknown sources in every supported language", () => {
    for (const language of supportedLanguages) {
      const messages = getMessages(language);
      expect(messages.unknownSource).not.toHaveLength(0);
      expect(messages.sourceBadge(messages.unknownSource)).toContain(
        messages.unknownSource
      );
    }
  });

  it("distinguishes a completed copy from post-processing failure", () => {
    for (const language of supportedLanguages) {
      const messages = getMessages(language);
      expect(
        messages.commandError(
          "restore_clipboard",
          "restore_post_processing_failed"
        )
      ).not.toBe(messages.commandError("restore_clipboard"));
    }
  });
});
