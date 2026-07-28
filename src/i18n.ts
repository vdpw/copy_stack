export const languagePreferences = ["system", "en", "zh-CN", "zh-TW"] as const;

export const supportedLanguages = ["en", "zh-CN", "zh-TW"] as const;

export type LanguagePreference = (typeof languagePreferences)[number];
export type SupportedLanguage = (typeof supportedLanguages)[number];

type EventType =
  | "text"
  | "rtf"
  | "html"
  | "file"
  | "folder"
  | "files"
  | "folders"
  | "files and folders"
  | "video"
  | "unsupported";

export interface Messages {
  settings: string;
  storedItems: string;
  storedItemsDescription: (maximum: number, current: number) => string;
  apply: string;
  storageLimitError: string;
  language: string;
  systemDefault: string;
  languageDescriptionSystem: (languageName: string) => string;
  languageDescriptionManual: string;
  compactMode: string;
  compactModeEnabled: string;
  compactModeDisabled: string;
  moveRestoredItemsToTop: string;
  restoreOrderingEnabled: string;
  restoreOrderingDisabled: string;
  showInMenuBar: string;
  menuBarEnabled: string;
  menuBarDisabled: string;
  clipboardHistory: string;
  recentEvents: string;
  historyDescription: string;
  refresh: string;
  clearAll: string;
  loadingHistory: string;
  emptyHistory: string;
  emptyHistoryCompact: string;
  emptyHistoryAll: string;
  formattedPreviewTitle: string;
  imageThumbnailAlt: (label: string) => string;
  videoCoverAlt: (label: string) => string;
  pngImage: string;
  image: string;
  video: string;
  text: string;
  textAndImage: string;
  eventTypes: Record<EventType, string>;
  fileFallbackName: (index: number) => string;
  folderFallbackName: (index: number) => string;
  moreItems: (count: number) => string;
  copiedToClipboard: string;
  restoreToClipboard: string;
  deleteItem: string;
  clipboardItemCopied: string;
  reduceHistory: string;
  reduceHistoryDescription: (
    current: number,
    next: number,
    deleteCount: number
  ) => string;
  cannotUndo: string;
  cancel: string;
  updating: string;
  deleteAndUpdate: string;
}

export const languageDisplayNames: Record<SupportedLanguage, string> = {
  en: "English",
  "zh-CN": "简体中文",
  "zh-TW": "繁體中文",
};

const englishClipCount = (count: number) =>
  `${count} clip${count === 1 ? "" : "s"}`;
const englishEventCount = (count: number) =>
  `${count} clipboard event${count === 1 ? "" : "s"}`;

const translations: Record<SupportedLanguage, Messages> = {
  en: {
    settings: "Settings",
    storedItems: "Stored items",
    storedItemsDescription: (maximum, current) =>
      `Keep the newest ${englishClipCount(maximum)}. Currently storing ${englishClipCount(current)}.`,
    apply: "Apply",
    storageLimitError: "Enter a whole number between 1 and 1000.",
    language: "Language",
    systemDefault: "System Default",
    languageDescriptionSystem: languageName =>
      `Use this computer's closest supported language (${languageName}).`,
    languageDescriptionManual:
      "Use this language in Copy Stack instead of the system language.",
    compactMode: "Compact mode",
    compactModeEnabled:
      "Only recognizable text is kept; image and file clips are ignored.",
    compactModeDisabled: "Keep all supported clipboard content and formatting.",
    moveRestoredItemsToTop: "Move restored items to top",
    restoreOrderingEnabled: "Restored clips refresh history order.",
    restoreOrderingDisabled: "Restored clips keep their current order.",
    showInMenuBar: "Show in menu bar",
    menuBarEnabled: "Recent clips are available from the tray menu.",
    menuBarDisabled: "The tray menu is hidden.",
    clipboardHistory: "Clipboard history",
    recentEvents: "Recent events",
    historyDescription:
      "Refresh the list, restore an item, or clear the local stack.",
    refresh: "Refresh",
    clearAll: "Clear all",
    loadingHistory: "Loading clipboard history...",
    emptyHistory: "No clipboard events yet",
    emptyHistoryCompact:
      "Start copying text and it will appear here and in the menu bar menu.",
    emptyHistoryAll:
      "Start copying text or files and they will appear here and in the menu bar menu.",
    formattedPreviewTitle: "Formatted clipboard preview",
    imageThumbnailAlt: label => `${label} thumbnail`,
    videoCoverAlt: label => `${label} video cover`,
    pngImage: "PNG image",
    image: "Image",
    video: "Video",
    text: "Text",
    textAndImage: "Text + image",
    eventTypes: {
      text: "Text",
      rtf: "RTF",
      html: "HTML",
      file: "File",
      folder: "Folder",
      files: "Files",
      folders: "Folders",
      "files and folders": "Files and folders",
      video: "Video",
      unsupported: "Unsupported content",
    },
    fileFallbackName: index => `File ${index}`,
    folderFallbackName: index => `Folder ${index}`,
    moreItems: count => ` + ${count} more`,
    copiedToClipboard: "Copied to clipboard",
    restoreToClipboard: "Restore to clipboard",
    deleteItem: "Delete item",
    clipboardItemCopied: "Clipboard item copied.",
    reduceHistory: "Reduce stored history?",
    reduceHistoryDescription: (current, next, deleteCount) =>
      `Changing the storage limit from ${current} to ${next} will remove ${englishEventCount(deleteCount)} from local storage, starting with the oldest.`,
    cannotUndo: "This action cannot be undone.",
    cancel: "Cancel",
    updating: "Updating...",
    deleteAndUpdate: "Delete and update",
  },
  "zh-CN": {
    settings: "设置",
    storedItems: "存储数量",
    storedItemsDescription: (maximum, current) =>
      `保留最新的 ${maximum} 条剪贴板内容，目前已存储 ${current} 条。`,
    apply: "应用",
    storageLimitError: "请输入 1 到 1000 之间的整数。",
    language: "语言",
    systemDefault: "跟随系统",
    languageDescriptionSystem: languageName =>
      `根据这台电脑的语言设置，使用最接近的受支持语言（${languageName}）。`,
    languageDescriptionManual: "在 Copy Stack 中使用此语言，不跟随系统语言。",
    compactMode: "精简模式",
    compactModeEnabled: "只保留可识别的文字；图片和文件不会被保存。",
    compactModeDisabled: "保留所有支持的剪贴板内容和格式。",
    moveRestoredItemsToTop: "将恢复的项目移到顶部",
    restoreOrderingEnabled: "恢复剪贴板内容后会更新历史记录顺序。",
    restoreOrderingDisabled: "恢复剪贴板内容后会保留当前顺序。",
    showInMenuBar: "在菜单栏中显示",
    menuBarEnabled: "可从菜单栏访问最近的剪贴板内容。",
    menuBarDisabled: "菜单栏图标已隐藏。",
    clipboardHistory: "剪贴板历史",
    recentEvents: "最近记录",
    historyDescription: "刷新列表、恢复项目或清空本地记录。",
    refresh: "刷新",
    clearAll: "全部清空",
    loadingHistory: "正在加载剪贴板历史...",
    emptyHistory: "暂无剪贴板记录",
    emptyHistoryCompact: "开始复制文字后，内容会显示在这里和菜单栏中。",
    emptyHistoryAll: "开始复制文字或文件后，内容会显示在这里和菜单栏中。",
    formattedPreviewTitle: "格式化剪贴板内容预览",
    imageThumbnailAlt: label => `${label}缩略图`,
    videoCoverAlt: label => `${label}视频封面`,
    pngImage: "PNG 图片",
    image: "图片",
    video: "视频",
    text: "文字",
    textAndImage: "文字和图片",
    eventTypes: {
      text: "文字",
      rtf: "RTF",
      html: "HTML",
      file: "文件",
      folder: "文件夹",
      files: "多个文件",
      folders: "多个文件夹",
      "files and folders": "文件和文件夹",
      video: "视频",
      unsupported: "不支持的内容",
    },
    fileFallbackName: index => `文件 ${index}`,
    folderFallbackName: index => `文件夹 ${index}`,
    moreItems: count => `，另有 ${count} 项`,
    copiedToClipboard: "已复制到剪贴板",
    restoreToClipboard: "恢复到剪贴板",
    deleteItem: "删除项目",
    clipboardItemCopied: "已复制到剪贴板。",
    reduceHistory: "减少存储的历史记录？",
    reduceHistoryDescription: (current, next, deleteCount) =>
      `将存储上限从 ${current} 改为 ${next}，会从最旧的记录开始删除本地存储中的 ${deleteCount} 条剪贴板记录。`,
    cannotUndo: "此操作无法撤销。",
    cancel: "取消",
    updating: "正在更新...",
    deleteAndUpdate: "删除并更新",
  },
  "zh-TW": {
    settings: "設定",
    storedItems: "儲存數量",
    storedItemsDescription: (maximum, current) =>
      `保留最新的 ${maximum} 筆剪貼簿內容，目前已儲存 ${current} 筆。`,
    apply: "套用",
    storageLimitError: "請輸入 1 到 1000 之間的整數。",
    language: "語言",
    systemDefault: "跟隨系統",
    languageDescriptionSystem: languageName =>
      `依照這台電腦的語言設定，使用最接近的支援語言（${languageName}）。`,
    languageDescriptionManual: "在 Copy Stack 中使用此語言，不跟隨系統語言。",
    compactMode: "精簡模式",
    compactModeEnabled: "只保留可辨識的文字；圖片和檔案不會被儲存。",
    compactModeDisabled: "保留所有支援的剪貼簿內容和格式。",
    moveRestoredItemsToTop: "將還原的項目移至頂端",
    restoreOrderingEnabled: "還原剪貼簿內容後會更新歷史記錄順序。",
    restoreOrderingDisabled: "還原剪貼簿內容後會保留目前順序。",
    showInMenuBar: "在選單列中顯示",
    menuBarEnabled: "可從選單列存取最近的剪貼簿內容。",
    menuBarDisabled: "選單列圖示已隱藏。",
    clipboardHistory: "剪貼簿歷史",
    recentEvents: "最近記錄",
    historyDescription: "重新整理列表、還原項目或清除本機記錄。",
    refresh: "重新整理",
    clearAll: "全部清除",
    loadingHistory: "正在載入剪貼簿歷史...",
    emptyHistory: "尚無剪貼簿記錄",
    emptyHistoryCompact: "開始複製文字後，內容會顯示在這裡和選單列中。",
    emptyHistoryAll: "開始複製文字或檔案後，內容會顯示在這裡和選單列中。",
    formattedPreviewTitle: "格式化剪貼簿內容預覽",
    imageThumbnailAlt: label => `${label}縮圖`,
    videoCoverAlt: label => `${label}影片封面`,
    pngImage: "PNG 圖片",
    image: "圖片",
    video: "影片",
    text: "文字",
    textAndImage: "文字和圖片",
    eventTypes: {
      text: "文字",
      rtf: "RTF",
      html: "HTML",
      file: "檔案",
      folder: "資料夾",
      files: "多個檔案",
      folders: "多個資料夾",
      "files and folders": "檔案和資料夾",
      video: "影片",
      unsupported: "不支援的內容",
    },
    fileFallbackName: index => `檔案 ${index}`,
    folderFallbackName: index => `資料夾 ${index}`,
    moreItems: count => `，另有 ${count} 個項目`,
    copiedToClipboard: "已複製到剪貼簿",
    restoreToClipboard: "還原至剪貼簿",
    deleteItem: "刪除項目",
    clipboardItemCopied: "已複製到剪貼簿。",
    reduceHistory: "減少儲存的歷史記錄？",
    reduceHistoryDescription: (current, next, deleteCount) =>
      `將儲存上限從 ${current} 改為 ${next}，會從最舊的記錄開始刪除本機儲存中的 ${deleteCount} 筆剪貼簿記錄。`,
    cannotUndo: "此操作無法還原。",
    cancel: "取消",
    updating: "正在更新...",
    deleteAndUpdate: "刪除並更新",
  },
};

export function isLanguagePreference(
  language: string
): language is LanguagePreference {
  return languagePreferences.some(candidate => candidate === language);
}

export function isSupportedLanguage(
  language: string
): language is SupportedLanguage {
  return supportedLanguages.some(candidate => candidate === language);
}

export function normalizeSupportedLanguage(
  locale: string
): SupportedLanguage | null {
  const subtags = locale.toLowerCase().replace(/_/g, "-").split("-");
  if (subtags[0] === "en") {
    return "en";
  }
  if (subtags[0] !== "zh") {
    return null;
  }
  if (subtags.includes("hant")) {
    return "zh-TW";
  }
  if (subtags.includes("hans")) {
    return "zh-CN";
  }
  if (subtags.some(subtag => ["tw", "hk", "mo"].includes(subtag))) {
    return "zh-TW";
  }
  return "zh-CN";
}

export function detectSystemLanguage(
  preferredLanguages?: readonly string[]
): SupportedLanguage {
  const candidates =
    preferredLanguages ??
    (typeof window === "undefined"
      ? []
      : [...window.navigator.languages, window.navigator.language]);

  for (const candidate of candidates) {
    const language = normalizeSupportedLanguage(candidate);
    if (language) {
      return language;
    }
  }

  return "en";
}

export function getMessages(language: SupportedLanguage): Messages {
  return translations[language];
}

export function getEventTypeLabel(
  messages: Messages,
  dataType: string
): string {
  if (dataType in messages.eventTypes) {
    return messages.eventTypes[dataType as EventType];
  }

  return dataType.toUpperCase();
}
