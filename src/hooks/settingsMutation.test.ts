import { describe, expect, it, vi } from "vitest";
import { runOptimisticMutation } from "./settingsMutation";

describe("runOptimisticMutation", () => {
  it("applies the authoritative result after an optimistic update", async () => {
    const apply = vi.fn();
    await expect(
      runOptimisticMutation({
        previous: { enabled: false },
        optimistic: { enabled: true },
        apply,
        mutate: async () => ({ enabled: true }),
      })
    ).resolves.toEqual({ enabled: true });
    expect(apply.mock.calls).toEqual([
      [{ enabled: true }],
      [{ enabled: true }],
    ]);
  });

  it("rolls back immediately when the mutation fails", async () => {
    const apply = vi.fn();
    const failure = new Error("safe test failure");
    await expect(
      runOptimisticMutation({
        previous: { enabled: false },
        optimistic: { enabled: true },
        apply,
        mutate: async () => {
          throw failure;
        },
      })
    ).rejects.toBe(failure);
    expect(apply.mock.calls).toEqual([
      [{ enabled: true }],
      [{ enabled: false }],
    ]);
  });
});
