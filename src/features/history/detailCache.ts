import type { HistoryDetail } from "../../types";

export type HistoryDetailLoader = (
  contentHash: string
) => Promise<HistoryDetail>;

export const HISTORY_DETAIL_CACHE_CAPACITY = 12;

export function canLoadHistoryDetail(
  compactMode: boolean,
  hasDetail: boolean
): boolean {
  return !compactMode && hasDetail;
}

interface PendingRequest {
  generation: number;
  requestId: number;
  promise: Promise<HistoryDetail | undefined>;
}

export class HistoryDetailCache {
  private readonly cache = new Map<string, HistoryDetail>();
  private readonly pending = new Map<string, PendingRequest>();
  private generation = 0;
  private nextRequestId = 0;

  constructor(
    private readonly loader: HistoryDetailLoader,
    private readonly maxEntries = HISTORY_DETAIL_CACHE_CAPACITY
  ) {}

  peek(contentHash: string): HistoryDetail | undefined {
    return this.cache.get(contentHash);
  }

  async load(contentHash: string): Promise<HistoryDetail | undefined> {
    const cached = this.cache.get(contentHash);
    if (cached) {
      this.cache.delete(contentHash);
      this.cache.set(contentHash, cached);
      return cached;
    }

    const activeRequest = this.pending.get(contentHash);
    if (activeRequest?.generation === this.generation) {
      return activeRequest.promise;
    }

    const generation = this.generation;
    const requestId = ++this.nextRequestId;
    const promise = this.loader(contentHash).then(detail => {
      const currentRequest = this.pending.get(contentHash);
      if (
        generation !== this.generation ||
        currentRequest?.requestId !== requestId
      ) {
        return undefined;
      }

      this.pending.delete(contentHash);
      this.cache.set(contentHash, detail);
      this.trim();
      return detail;
    });

    while (this.pending.size >= Math.max(1, this.maxEntries)) {
      const oldestPending = this.pending.keys().next().value;
      if (typeof oldestPending !== "string") {
        break;
      }
      this.pending.delete(oldestPending);
    }
    this.pending.set(contentHash, { generation, requestId, promise });

    try {
      return await promise;
    } catch (error) {
      const currentRequest = this.pending.get(contentHash);
      if (currentRequest?.requestId === requestId) {
        this.pending.delete(contentHash);
      }
      throw error;
    }
  }

  remove(contentHash: string): void {
    this.cache.delete(contentHash);
    this.pending.delete(contentHash);
  }

  retain(contentHashes: ReadonlySet<string>): void {
    for (const contentHash of this.cache.keys()) {
      if (!contentHashes.has(contentHash)) {
        this.cache.delete(contentHash);
      }
    }
    for (const contentHash of this.pending.keys()) {
      if (!contentHashes.has(contentHash)) {
        this.pending.delete(contentHash);
      }
    }
  }

  reset(): void {
    this.generation += 1;
    this.cache.clear();
    this.pending.clear();
  }

  get size(): number {
    return this.cache.size;
  }

  private trim(): void {
    while (this.cache.size > Math.max(1, this.maxEntries)) {
      const oldestKey = this.cache.keys().next().value;
      if (typeof oldestKey !== "string") {
        break;
      }
      this.cache.delete(oldestKey);
    }
  }
}
