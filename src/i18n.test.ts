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

  it("explains failed moves and unchanged storage in every supported language", () => {
    for (const language of supportedLanguages) {
      const messages = getMessages(language);
      const conflict = messages.commandError(
        "move_storage",
        "storage_destination_exists"
      );
      const permission = messages.commandError(
        "move_storage",
        "storage_permission_denied"
      );
      expect(conflict).not.toBe(permission);
      expect(conflict).toMatch(/unchanged|原目录|原目錄/);
      expect(permission).toMatch(/unchanged|原目录|原目錄/);
      expect(
        messages.commandError("move_storage", "storage_move_failed")
      ).toMatch(/unchanged|原目录|原目錄/);
    }
  });
});
