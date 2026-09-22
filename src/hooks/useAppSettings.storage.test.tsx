// @vitest-environment jsdom

import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../types";
import { useAppSettings } from "./useAppSettings";
import type { AppSettingsController } from "./useAppSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const initial: AppSettings = {
  storage_directory: "/private/example/ClipEcho",
  theme: "system",
  max_items: 100,
  max_history_bytes: 268435456,
  show_in_menu_bar: true,
  menu_bar_item_limit: 0,
  move_restored_item_to_top: false,
  compact_mode: false,
  language: "en",
  resolved_language: "en",
  history_count: 12,
  history_bytes: 1024,
  history_limit_bytes: 268435456,
  max_event_bytes: 33554432,
};
const moved = {
  ...initial,
  storage_directory: "/private/example/New ClipEcho Folder",
};
const roots: ReturnType<typeof createRoot>[] = [];

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function mountController() {
  let controller: AppSettingsController;
  function Harness() {
    controller = useAppSettings(false);
    return null;
  }
  const root = createRoot(document.createElement("div"));
  roots.push(root);
  flushSync(() => root.render(<Harness />));
  await vi.waitFor(() => expect(controller?.settings).toEqual(initial));
  return () => controller;
}

afterEach(() => {
  roots.splice(0).forEach(root => flushSync(() => root.unmount()));
  vi.clearAllMocks();
});

describe("storage directory changes", () => {
  it("waits for the move to commit and blocks repeated or competing mutations", async () => {
    const picker = deferred<string | null>();
    const transfer = deferred<AppSettings>();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_app_settings") return initial;
      if (command === "choose_storage_directory") return picker.promise;
      if (command === "set_storage_directory") {
        expect(args).toEqual({ directory: moved.storage_directory });
        return transfer.promise;
      }
      throw new Error(`unexpected command: ${command}`);
    });
    const controller = await mountController();
    let change!: Promise<void>;
    flushSync(() => {
      change = controller().changeStorageDirectory();
      void controller().changeStorageDirectory();
      void controller().updateTheme("dark");
      void controller().updateAutostart(true);
    });
    expect(controller().updating).toBe(true);
    expect(controller().movingStorage).toBe(false);
    expect(controller().settings).toEqual(initial);
    expect(vi.mocked(invoke).mock.calls.map(call => call[0])).toEqual([
      "get_app_settings",
      "choose_storage_directory",
    ]);

    picker.resolve(moved.storage_directory);
    await vi.waitFor(() => expect(controller().movingStorage).toBe(true));
    expect(controller().settings).toEqual(initial);
    await controller().loadSettings();
    expect(invoke).toHaveBeenCalledTimes(3);
    transfer.resolve(moved);
    await change;
    await vi.waitFor(() => expect(controller().settings).toEqual(moved));
    expect(controller().updating).toBe(false);
    expect(controller().movingStorage).toBe(false);
    expect(controller().error).toBeNull();
  });

  it.each([
    "storage_destination_exists",
    "storage_permission_denied",
    "storage_invalid_directory",
    "storage_move_failed",
  ] as const)("retains the previous location and reports %s", async code => {
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "get_app_settings") return initial;
      if (command === "choose_storage_directory")
        return moved.storage_directory;
      if (command === "set_storage_directory") {
        throw {
          code,
          operation: "move_storage",
          retryable: false,
          path: "/private/do-not-expose-in-errors",
        };
      }
      throw new Error(`unexpected command: ${command}`);
    });
    const controller = await mountController();
    await controller().changeStorageDirectory();
    await vi.waitFor(() => expect(controller().error?.code).toBe(code));
    expect(controller().settings).toEqual(initial);
    expect(controller().error?.operation).toBe("move_storage");
    expect(JSON.stringify(controller().error)).not.toContain("do-not-expose");
    expect(controller().updating).toBe(false);
    expect(controller().movingStorage).toBe(false);
  });

  it.each([null, initial.storage_directory])(
    "leaves settings unchanged when selection is %s",
    async selection => {
      vi.mocked(invoke).mockImplementation(async command => {
        if (command === "get_app_settings") return initial;
        if (command === "choose_storage_directory") return selection;
        throw new Error(`unexpected command: ${command}`);
      });
      const controller = await mountController();
      await controller().changeStorageDirectory();
      await vi.waitFor(() => expect(controller().updating).toBe(false));
      expect(controller().settings).toEqual(initial);
      expect(controller().error).toBeNull();
      expect(invoke).toHaveBeenCalledTimes(2);
    }
  );

  it("ignores a settings response started before the move", async () => {
    const previousLoad = deferred<AppSettings>();
    let loads = 0;
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "get_app_settings") {
        loads += 1;
        return loads === 1 ? initial : previousLoad.promise;
      }
      if (command === "choose_storage_directory")
        return moved.storage_directory;
      if (command === "set_storage_directory") return moved;
      throw new Error(`unexpected command: ${command}`);
    });
    const controller = await mountController();
    const oldRequest = controller().loadSettings();
    await controller().changeStorageDirectory();
    await vi.waitFor(() => expect(controller().settings).toEqual(moved));
    previousLoad.resolve(initial);
    await oldRequest;
    await vi.waitFor(() => expect(controller().loading).toBe(false));
    expect(controller().settings).toEqual(moved);
  });
});
