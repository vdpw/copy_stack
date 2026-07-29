import type { HistorySummary } from "../types";

const fileDisplayFormat = "copy_stack.file-items.v1";
const defaultDisplayWidth = 40;
const truncationSuffix = "...";
const pngSignature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

export const MAX_PREVIEW_IMAGE_BYTES = 4 * 1024 * 1024;

export interface FileDisplayItem {
  type: string;
  name: string;
}

interface FileDisplayPayload {
  format: string;
  items: FileDisplayItem[];
}

export function getCharacterDisplayWidth(character: string): number {
  if (
    /[\u1100-\u115F\u2329\u232A\u2E80-\uA4CF\uAC00-\uD7A3\uF900-\uFAFF\uFE10-\uFE19\uFE30-\uFE6F\uFF00-\uFF60\uFFE0-\uFFE6\u{1F300}-\u{1FAFF}]/u.test(
      character
    )
  ) {
    return 2;
  }

  return 1;
}

export function getDisplayWidth(content: string): number {
  return Array.from(content).reduce(
    (width, character) => width + getCharacterDisplayWidth(character),
    0
  );
}

export function truncateContent(
  content: string,
  maxWidth = defaultDisplayWidth
): string {
  const flattened = content.replace(/\s+/g, " ").trim();
  if (getDisplayWidth(flattened) <= maxWidth) {
    return flattened;
  }

  const availableWidth = Math.max(
    0,
    maxWidth - getDisplayWidth(truncationSuffix)
  );
  let truncated = "";
  let currentWidth = 0;

  for (const character of Array.from(flattened)) {
    const characterWidth = getCharacterDisplayWidth(character);
    if (currentWidth + characterWidth > availableWidth) {
      break;
    }

    truncated += character;
    currentWidth += characterWidth;
  }

  return `${truncated}${truncationSuffix}`;
}

export function parseFileDisplay(text: string): FileDisplayItem[] | null {
  try {
    const parsed = JSON.parse(text) as Partial<FileDisplayPayload>;
    if (parsed.format !== fileDisplayFormat || !Array.isArray(parsed.items)) {
      return null;
    }

    const items = parsed.items.filter((item): item is FileDisplayItem => {
      const candidate = item as Partial<FileDisplayItem> | null;
      return (
        typeof candidate?.type === "string" &&
        typeof candidate.name === "string"
      );
    });
    return items.length > 0 ? items : null;
  } catch {
    return null;
  }
}

export function decodeSummaryDisplay(
  summary: HistorySummary,
  fallbackLabel: string,
  videoLabel: string
): string {
  const text = new TextDecoder().decode(new Uint8Array(summary.display));
  if (text.includes("\uFFFD")) {
    return fallbackLabel;
  }
  if (summary.data_type === "video" && text === "Video") {
    return videoLabel;
  }
  return text;
}

export function isPngBytes(bytes: readonly number[]): boolean {
  return (
    bytes.length >= pngSignature.length &&
    pngSignature.every((byte, index) => bytes[index] === byte)
  );
}

export function isSafePreviewImage(
  data: readonly number[],
  mediaType: string
): boolean {
  return (
    data.length > 0 &&
    data.length <= MAX_PREVIEW_IMAGE_BYTES &&
    /^image\/(?:bmp|gif|jpeg|jpg|png|webp)$/i.test(mediaType)
  );
}

export function formatBytes(bytes: number, locale: string): string {
  const safeBytes = Number.isFinite(bytes) ? Math.max(0, bytes) : 0;
  if (safeBytes < 1024) {
    return `${Math.round(safeBytes)} B`;
  }

  const units = ["KB", "MB", "GB", "TB"];
  let value = safeBytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }

  return `${new Intl.NumberFormat(locale, {
    maximumFractionDigits: value >= 10 ? 1 : 2,
  }).format(value)} ${units[unitIndex]}`;
}

export function sourceDisplayName(bundleId: string): string {
  const knownNames: Record<string, string> = {
    "com.apple.finder": "Finder",
    "com.apple.Safari": "Safari",
    "com.google.Chrome": "Chrome",
    "com.microsoft.VSCode": "Visual Studio Code",
    "com.tinyspeck.slackmacgap": "Slack",
  };
  return knownNames[bundleId] ?? bundleId;
}
