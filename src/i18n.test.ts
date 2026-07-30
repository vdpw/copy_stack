import { describe, expect, it } from "vitest";
import { getMessages, supportedLanguages } from "./i18n";

describe("metadata and error localization", () => {
  it("localizes remote clipboard metadata in every supported language", () => {
    for (const language of supportedLanguages) {
      const messages = getMessages(language);
      expect(messages.remoteClipboard).not.toHaveLength(0);
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
