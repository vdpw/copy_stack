import type { ErrorCode, Operation } from "./types";

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
  backToHistory: string;
  backToHistoryShort: string;
  loadingSettings: string;
  starting: string;
  storedItems: string;
  settingHelp: (setting: string) => string;
  storedItemsHelp: string;
  historyBudgetHelp: (maximumEventSize: string) => string;
  storedItemsDescription: (maximum: number, current: number) => string;
  historyStorageUsage: (current: string, maximum: string) => string;
  maximumEventSize: (maximum: string) => string;
  historyBudget: string;
  historyBudgetDescription: string;
  historyBudgetError: string;
  apply: string;
  storageLimitError: string;
  language: string;
  systemDefault: string;
  languageDescriptionSystem: (languageName: string) => string;
  languageDescriptionManual: string;
  compactMode: string;
  compactModeEnabled: string;
  compactModeDisabled: string;
  launchAtLogin: string;
  launchAtLoginEnabled: string;
  launchAtLoginDisabled: string;
  launchAtLoginLoading: string;
  launchAtLoginReadError: string;
  launchAtLoginUpdateError: string;
  moveRestoredItemsToTop: string;
  restoreOrderingEnabled: string;
  restoreOrderingDisabled: string;
  showInMenuBar: string;
  menuBarEnabled: string;
  menuBarDisabled: string;
  menuBarItemLimit: string;
  menuBarItemLimitDescription: (limit: number) => string;
  menuBarItemLimitError: string;
  clipboardHistory: string;
  clearAll: string;
  clearHistoryDescription: (count: number) => string;
  clearHistoryConfirmationTitle: string;
  clearHistoryConfirmationDescription: (count: number) => string;
  clearingHistory: string;
  loadMore: string;
  loadingMore: string;
  loadedHistoryCount: (loaded: number, total: number) => string;
  searchClipboardHistory: string;
  clearSearch: string;
  searchMatch: string;
  loadedSearchCount: (loaded: number, total: number) => string;
  noSearchResults: string;
  noSearchResultsDescription: (query: string) => string;
  loadingHistory: string;
  loadingDetail: string;
  detailUnavailable: string;
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
  restoringToClipboard: string;
  deleteItem: string;
  recentHistory: string;
  generalSettings: string;
  generalSettingsDescription: string;
  appearanceSettings: string;
  appearanceSettingsDescription: string;
  theme: string;
  themeDescription: string;
  themeSystem: string;
  themeLight: string;
  themeDark: string;
  menuBarSettings: string;
  menuBarSettingsDescription: string;
  clipboardSettings: string;
  clipboardSettingsDescription: string;
  storageSettings: string;
  behaviorSettings: string;
  pinItem: string;
  unpinItem: string;
  pinned: string;
  pinUpdated: string;
  pinRetentionHint: string;
  clipboardItemCopied: string;
  remoteClipboard: string;
  previewTruncated: string;
  captureRejected: string;
  retry: string;
  dismiss: string;
  diagnosticDetails: string;
  copyDiagnostic: string;
  diagnosticLoading: string;
  diagnosticUnavailable: string;
  diagnosticCopied: string;
  diagnosticCopyFailed: string;
  commandError: (operation: Operation, code?: ErrorCode) => string;
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

const englishOperationErrors: Record<Operation, string> = {
  startup: "ClipEcho could not finish starting.",
  capture_clipboard: "This clipboard item could not be saved.",
  load_history: "Clipboard history could not be loaded.",
  load_history_detail: "This clipboard preview could not be loaded.",
  restore_clipboard: "This item could not be restored to the clipboard.",
  delete_history: "This clipboard item could not be deleted.",
  pin_history: "The pin status could not be updated.",
  clear_history: "Clipboard history could not be cleared.",
  load_settings: "Settings could not be loaded.",
  update_settings:
    "The setting could not be updated. Its saved value was restored.",
  update_autostart: "The login startup setting could not be updated.",
  write_history_mirror: "The optional history export could not be updated.",
};

const simplifiedChineseOperationErrors: Record<Operation, string> = {
  startup: "ClipEcho 无法完成启动。",
  capture_clipboard: "无法保存此剪贴板内容。",
  load_history: "无法加载剪贴板历史。",
  load_history_detail: "无法加载此剪贴板预览。",
  restore_clipboard: "无法将此项目恢复到剪贴板。",
  delete_history: "无法删除此剪贴板项目。",
  pin_history: "无法更新固定状态。",
  clear_history: "无法清空剪贴板历史。",
  load_settings: "无法加载设置。",
  update_settings: "无法更新设置，已恢复保存的值。",
  update_autostart: "无法更新登录启动设置。",
  write_history_mirror: "无法更新可选的历史记录导出。",
};

const traditionalChineseOperationErrors: Record<Operation, string> = {
  startup: "ClipEcho 無法完成啟動。",
  capture_clipboard: "無法儲存此剪貼簿內容。",
  load_history: "無法載入剪貼簿歷史。",
  load_history_detail: "無法載入此剪貼簿預覽。",
  restore_clipboard: "無法將此項目還原至剪貼簿。",
  delete_history: "無法刪除此剪貼簿項目。",
  pin_history: "無法更新固定狀態。",
  clear_history: "無法清除剪貼簿歷史。",
  load_settings: "無法載入設定。",
  update_settings: "無法更新設定，已還原儲存的值。",
  update_autostart: "無法更新登入啟動設定。",
  write_history_mirror: "無法更新選用的歷史記錄匯出。",
};

const translations: Record<SupportedLanguage, Messages> = {
  en: {
    settings: "Settings",
    backToHistory: "Back to clipboard history",
    backToHistoryShort: "Back to History",
    loadingSettings: "Loading settings...",
    starting: "Starting ClipEcho...",
    storedItems: "Stored items",
    settingHelp: setting => `${setting} help`,
    storedItemsHelp:
      "Over the item limit, the oldest unpinned clips are removed. Pinned clips are always kept.",
    historyBudgetHelp: maximum =>
      `Over the storage limit, the oldest unpinned clips are removed. Pinned clips stay; each clip can use up to ${maximum}.`,
    storedItemsDescription: (maximum, current) =>
      `Keep up to ${englishClipCount(maximum)}, prioritizing pinned clips. Currently storing ${englishClipCount(current)}.`,
    historyStorageUsage: (current, maximum) =>
      `History uses ${current} of the ${maximum} local storage budget.`,
    maximumEventSize: maximum =>
      `A single clipboard item can use up to ${maximum}.`,
    historyBudget: "History storage budget",
    historyBudgetDescription:
      "Maximum local history size in MiB (16–4096). Oldest unpinned items are removed first.",
    historyBudgetError: "Enter a whole number from 16 to 4096 MiB.",
    apply: "Apply",
    storageLimitError: "Enter a whole number between 1 and 1000.",
    language: "Language",
    systemDefault: "System Default",
    languageDescriptionSystem: languageName =>
      `Use this computer's closest supported language (${languageName}).`,
    languageDescriptionManual:
      "Use this language in ClipEcho instead of the system language.",
    compactMode: "Compact mode",
    compactModeEnabled:
      "Only recognizable text is kept; image and file clips are ignored.",
    compactModeDisabled: "Keep all supported clipboard content and formatting.",
    launchAtLogin: "Launch at login",
    launchAtLoginEnabled: "Starts quietly when you sign in.",
    launchAtLoginDisabled: "Does not start automatically.",
    launchAtLoginLoading: "Checking login startup status...",
    launchAtLoginReadError:
      "Could not verify the login startup setting. Try reopening Settings.",
    launchAtLoginUpdateError:
      "Could not update the login startup setting. The previous setting was restored.",
    moveRestoredItemsToTop: "Move restored items to top",
    restoreOrderingEnabled: "Restored clips refresh history order.",
    restoreOrderingDisabled: "Restored clips keep their current order.",
    showInMenuBar: "Show in menu bar",
    menuBarEnabled: "Recent clips are available from the tray menu.",
    menuBarDisabled: "The tray menu is hidden.",
    menuBarItemLimit: "Tray menu items",
    menuBarItemLimitDescription: limit =>
      limit === 0
        ? "Pinned clips appear first. Enter 0 for all (up to 1000), or set a limit from 1 to 1000."
        : `Show up to ${englishClipCount(limit)}, with pinned clips first. Enter 0 to show all (up to 1000).`,
    menuBarItemLimitError: "Enter 0 for all, or a whole number from 1 to 1000.",
    clipboardHistory: "Clipboard history",
    clearAll: "Clear unpinned",
    clearHistoryDescription: count =>
      count === 0
        ? "There is no clipboard history to clear."
        : `There are ${englishEventCount(count)} on this Mac. Clearing history deletes only unpinned items.`,
    clearHistoryConfirmationTitle: "Clear unpinned clipboard history?",
    clearHistoryConfirmationDescription: count =>
      `Of the ${englishEventCount(count)} on this Mac, only unpinned items will be permanently deleted. Pinned items will be kept.`,
    clearingHistory: "Clearing...",
    loadMore: "Load more",
    loadingMore: "Loading more...",
    loadedHistoryCount: (loaded, total) =>
      `Showing ${loaded} of ${englishClipCount(total)}.`,
    searchClipboardHistory: "Search clipboard history",
    clearSearch: "Clear search",
    searchMatch: "Matching text",
    loadedSearchCount: (loaded, total) =>
      `Showing ${loaded} of ${total} matching clips.`,
    noSearchResults: "No matching clips",
    noSearchResultsDescription: query =>
      `No clipboard history matches “${query}”.`,
    loadingHistory: "Loading clipboard history...",
    loadingDetail: "Loading full preview...",
    detailUnavailable: "The full preview is unavailable.",
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
    restoringToClipboard: "Restoring to clipboard...",
    deleteItem: "Delete item",
    pinItem: "Pin item",
    recentHistory: "Recent",
    generalSettings: "General",
    generalSettingsDescription: "Language and startup preferences.",
    appearanceSettings: "Appearance",
    appearanceSettingsDescription: "Choose how ClipEcho looks.",
    theme: "Color theme",
    themeDescription: "Choose a light or dark appearance, or follow your Mac.",
    themeSystem: "System",
    themeLight: "Light",
    themeDark: "Dark",
    menuBarSettings: "Menu Bar",
    menuBarSettingsDescription:
      "Quick access to clipboard history from your menu bar.",
    clipboardSettings: "Clipboard",
    clipboardSettingsDescription:
      "Manage capture behavior and retained history.",
    storageSettings: "Storage",
    behaviorSettings: "Capture & behavior",
    unpinItem: "Unpin item",
    pinned: "Pinned",
    pinUpdated: "Pin status updated.",
    pinRetentionHint:
      "Pinned items are kept when clearing history or reaching storage limits. If pinned items alone exceed a limit, they are still kept. Unpinning applies the current limits immediately.",
    clipboardItemCopied: "Clipboard item copied.",
    remoteClipboard: "From another device",
    previewTruncated: "Summary shortened",
    captureRejected:
      "A clipboard item was not saved because it exceeded a safety limit or requested privacy.",
    retry: "Retry",
    dismiss: "Dismiss",
    diagnosticDetails: "Safe diagnostic details",
    copyDiagnostic: "Copy diagnostic",
    diagnosticLoading: "Loading safe diagnostic details...",
    diagnosticUnavailable: "Safe diagnostic details are unavailable.",
    diagnosticCopied: "Diagnostic copied.",
    diagnosticCopyFailed: "The diagnostic could not be copied.",
    commandError: (operation, code) =>
      code === "restore_post_processing_failed"
        ? "Copied, but the history or menu refresh failed."
        : englishOperationErrors[operation],
    reduceHistory: "Reduce stored history?",
    reduceHistoryDescription: (current, next, deleteCount) =>
      `Changing the storage limit from ${current} to ${next} will remove up to ${englishEventCount(deleteCount)}, starting with the oldest unpinned items. Pinned items will be kept.`,
    cannotUndo: "This action cannot be undone.",
    cancel: "Cancel",
    updating: "Updating...",
    deleteAndUpdate: "Delete and update",
  },
  "zh-CN": {
    settings: "设置",
    backToHistory: "返回剪贴板历史",
    backToHistoryShort: "返回主界面",
    loadingSettings: "正在加载设置...",
    starting: "正在启动 ClipEcho...",
    storedItems: "存储数量",
    settingHelp: setting => `${setting}说明`,
    storedItemsHelp: "超出数量上限时，清理最旧的未固定记录。固定记录始终保留。",
    historyBudgetHelp: maximum =>
      `超出容量时，清理最旧的未固定记录。固定记录保留；单条最多 ${maximum}。`,
    storedItemsDescription: (maximum, current) =>
      `数量上限 ${maximum} 条，优先保留固定项目。目前已存储 ${current} 条。`,
    historyStorageUsage: (current, maximum) =>
      `历史记录已使用 ${current}，本地存储预算为 ${maximum}。`,
    maximumEventSize: maximum => `单条剪贴板内容最多可使用 ${maximum}。`,
    historyBudget: "历史记录存储预算",
    historyBudgetDescription:
      "本地历史记录的最大大小（MiB，16–4096）。超出后会先删除最旧的未固定项目。",
    historyBudgetError: "请输入 16 到 4096 之间的整数（MiB）。",
    apply: "应用",
    storageLimitError: "请输入 1 到 1000 之间的整数。",
    language: "语言",
    systemDefault: "跟随系统",
    languageDescriptionSystem: languageName =>
      `根据这台电脑的语言设置，使用最接近的受支持语言（${languageName}）。`,
    languageDescriptionManual: "在 ClipEcho 中使用此语言，不跟随系统语言。",
    compactMode: "精简模式",
    compactModeEnabled: "只保留可识别的文字；图片和文件不会被保存。",
    compactModeDisabled: "保留所有支持的剪贴板内容和格式。",
    launchAtLogin: "登录时启动",
    launchAtLoginEnabled: "登录后会静默启动。",
    launchAtLoginDisabled: "不会自动启动。",
    launchAtLoginLoading: "正在检查登录启动状态...",
    launchAtLoginReadError: "无法确认登录启动设置。请重新打开“设置”后再试。",
    launchAtLoginUpdateError: "无法更新登录启动设置，已恢复之前的设置。",
    moveRestoredItemsToTop: "将恢复的项目移到顶部",
    restoreOrderingEnabled: "恢复剪贴板内容后会更新历史记录顺序。",
    restoreOrderingDisabled: "恢复剪贴板内容后会保留当前顺序。",
    showInMenuBar: "在菜单栏中显示",
    menuBarEnabled: "可从菜单栏访问最近的剪贴板内容。",
    menuBarDisabled: "菜单栏图标已隐藏。",
    menuBarItemLimit: "托盘菜单条目数",
    menuBarItemLimitDescription: limit =>
      limit === 0
        ? "固定项目优先。输入 0 展示全部（最多 1000 条），也可设置 1–1000。"
        : `最多展示 ${limit} 个项目，固定项目优先。输入 0 展示全部（最多 1000 条）。`,
    menuBarItemLimitError: "请输入 0（全部）或 1–1000 的整数。",
    clipboardHistory: "剪贴板历史",
    clearAll: "清空未固定",
    clearHistoryDescription: count =>
      count === 0
        ? "目前没有可清空的剪贴板历史。"
        : `当前共 ${count} 条记录。清空时只删除未固定的项目，固定项目会保留。`,
    clearHistoryConfirmationTitle: "清空未固定的剪贴板历史？",
    clearHistoryConfirmationDescription: count =>
      `当前共 ${count} 条记录。清空时只删除未固定的项目，固定项目会保留。`,
    clearingHistory: "正在清空...",
    loadMore: "加载更多",
    loadingMore: "正在加载...",
    loadedHistoryCount: (loaded, total) => `已显示 ${loaded}/${total} 条。`,
    searchClipboardHistory: "搜索剪贴板历史",
    clearSearch: "清除搜索",
    searchMatch: "匹配内容",
    loadedSearchCount: (loaded, total) =>
      `已显示 ${loaded}/${total} 条匹配记录。`,
    noSearchResults: "没有匹配的记录",
    noSearchResultsDescription: query =>
      `剪贴板历史中没有匹配“${query}”的内容。`,
    loadingHistory: "正在加载剪贴板历史...",
    loadingDetail: "正在加载完整预览...",
    detailUnavailable: "无法显示完整预览。",
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
    restoringToClipboard: "正在恢复到剪贴板...",
    deleteItem: "删除项目",
    pinItem: "固定项目",
    recentHistory: "最近记录",
    generalSettings: "通用",
    generalSettingsDescription: "管理语言与登录启动偏好。",
    appearanceSettings: "外观",
    appearanceSettingsDescription: "选择 ClipEcho 的窗口外观。",
    theme: "颜色主题",
    themeDescription: "选择浅色、深色，或跟随 Mac 的系统外观。",
    themeSystem: "跟随系统",
    themeLight: "浅色",
    themeDark: "深色",
    menuBarSettings: "菜单栏",
    menuBarSettingsDescription: "从菜单栏快速访问剪贴板历史。",
    clipboardSettings: "剪贴板",
    clipboardSettingsDescription: "管理采集行为与历史保留规则。",
    storageSettings: "存储",
    behaviorSettings: "采集与行为",
    unpinItem: "取消固定",
    pinned: "已固定",
    pinUpdated: "固定状态已更新。",
    pinRetentionHint:
      "固定项目不会被清空或自动淘汰；仅固定项目就超出数量或容量上限时，仍会保留。取消固定后立即应用当前上限。",
    clipboardItemCopied: "已复制到剪贴板。",
    remoteClipboard: "来自其他设备",
    previewTruncated: "摘要已缩短",
    captureRejected:
      "由于内容超出安全限制或请求保护隐私，一条剪贴板内容未被保存。",
    retry: "重试",
    dismiss: "关闭",
    diagnosticDetails: "安全诊断信息",
    copyDiagnostic: "复制诊断信息",
    diagnosticLoading: "正在加载安全诊断信息...",
    diagnosticUnavailable: "无法获取安全诊断信息。",
    diagnosticCopied: "诊断信息已复制。",
    diagnosticCopyFailed: "无法复制诊断信息。",
    commandError: (operation, code) =>
      code === "restore_post_processing_failed"
        ? "已复制，但历史记录或菜单刷新失败。"
        : simplifiedChineseOperationErrors[operation],
    reduceHistory: "减少存储的历史记录？",
    reduceHistoryDescription: (current, next, deleteCount) =>
      `将存储上限从 ${current} 改为 ${next}，将从最旧的未固定项目开始，最多删除 ${deleteCount} 条记录，固定项目会保留。`,
    cannotUndo: "此操作无法撤销。",
    cancel: "取消",
    updating: "正在更新...",
    deleteAndUpdate: "删除并更新",
  },
  "zh-TW": {
    settings: "設定",
    backToHistory: "返回剪貼簿歷史",
    backToHistoryShort: "返回主畫面",
    loadingSettings: "正在載入設定...",
    starting: "正在啟動 ClipEcho...",
    storedItems: "儲存數量",
    settingHelp: setting => `${setting}說明`,
    storedItemsHelp: "超出數量上限時，清理最舊的未固定記錄。固定記錄始終保留。",
    historyBudgetHelp: maximum =>
      `超出容量時，清理最舊的未固定記錄。固定記錄保留；單筆最多 ${maximum}。`,
    storedItemsDescription: (maximum, current) =>
      `數量上限 ${maximum} 筆，優先保留固定項目。目前已儲存 ${current} 筆。`,
    historyStorageUsage: (current, maximum) =>
      `歷史記錄已使用 ${current}，本機儲存預算為 ${maximum}。`,
    maximumEventSize: maximum => `單筆剪貼簿內容最多可使用 ${maximum}。`,
    historyBudget: "歷史記錄儲存預算",
    historyBudgetDescription:
      "本機歷史記錄的最大大小（MiB，16–4096）。超出後會先刪除最舊的未固定項目。",
    historyBudgetError: "請輸入 16 到 4096 之間的整數（MiB）。",
    apply: "套用",
    storageLimitError: "請輸入 1 到 1000 之間的整數。",
    language: "語言",
    systemDefault: "跟隨系統",
    languageDescriptionSystem: languageName =>
      `依照這台電腦的語言設定，使用最接近的支援語言（${languageName}）。`,
    languageDescriptionManual: "在 ClipEcho 中使用此語言，不跟隨系統語言。",
    compactMode: "精簡模式",
    compactModeEnabled: "只保留可辨識的文字；圖片和檔案不會被儲存。",
    compactModeDisabled: "保留所有支援的剪貼簿內容和格式。",
    launchAtLogin: "登入時啟動",
    launchAtLoginEnabled: "登入後會靜默啟動。",
    launchAtLoginDisabled: "不會自動啟動。",
    launchAtLoginLoading: "正在檢查登入啟動狀態...",
    launchAtLoginReadError: "無法確認登入啟動設定。請重新開啟「設定」後再試。",
    launchAtLoginUpdateError: "無法更新登入啟動設定，已還原先前的設定。",
    moveRestoredItemsToTop: "將還原的項目移至頂端",
    restoreOrderingEnabled: "還原剪貼簿內容後會更新歷史記錄順序。",
    restoreOrderingDisabled: "還原剪貼簿內容後會保留目前順序。",
    showInMenuBar: "在選單列中顯示",
    menuBarEnabled: "可從選單列存取最近的剪貼簿內容。",
    menuBarDisabled: "選單列圖示已隱藏。",
    menuBarItemLimit: "選單列選單項目數",
    menuBarItemLimitDescription: limit =>
      limit === 0
        ? "固定項目優先。輸入 0 顯示全部（最多 1000 筆），也可設定 1–1000。"
        : `最多顯示 ${limit} 個項目，固定項目優先。輸入 0 顯示全部（最多 1000 筆）。`,
    menuBarItemLimitError: "請輸入 0（全部）或 1–1000 的整數。",
    clipboardHistory: "剪貼簿歷史",
    clearAll: "清除未固定",
    clearHistoryDescription: count =>
      count === 0
        ? "目前沒有可清除的剪貼簿歷史。"
        : `目前共 ${count} 筆記錄。只刪除未固定的項目，固定項目會保留。`,
    clearHistoryConfirmationTitle: "清除未固定的剪貼簿歷史？",
    clearHistoryConfirmationDescription: count =>
      `目前共 ${count} 筆記錄。只刪除未固定的項目，固定項目會保留。`,
    clearingHistory: "正在清除...",
    loadMore: "載入更多",
    loadingMore: "正在載入...",
    loadedHistoryCount: (loaded, total) => `已顯示 ${loaded}/${total} 筆。`,
    searchClipboardHistory: "搜尋剪貼簿歷史",
    clearSearch: "清除搜尋",
    searchMatch: "符合內容",
    loadedSearchCount: (loaded, total) =>
      `已顯示 ${loaded}/${total} 筆符合記錄。`,
    noSearchResults: "沒有符合的記錄",
    noSearchResultsDescription: query =>
      `剪貼簿歷史中沒有符合「${query}」的內容。`,
    loadingHistory: "正在載入剪貼簿歷史...",
    loadingDetail: "正在載入完整預覽...",
    detailUnavailable: "無法顯示完整預覽。",
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
    restoringToClipboard: "正在還原至剪貼簿...",
    deleteItem: "刪除項目",
    pinItem: "固定項目",
    recentHistory: "最近記錄",
    generalSettings: "一般",
    generalSettingsDescription: "管理語言與登入啟動偏好。",
    appearanceSettings: "外觀",
    appearanceSettingsDescription: "選擇 ClipEcho 的視窗外觀。",
    theme: "顏色主題",
    themeDescription: "選擇淺色、深色，或跟隨 Mac 的系統外觀。",
    themeSystem: "跟隨系統",
    themeLight: "淺色",
    themeDark: "深色",
    menuBarSettings: "選單列",
    menuBarSettingsDescription: "從選單列快速存取剪貼簿歷史。",
    clipboardSettings: "剪貼簿",
    clipboardSettingsDescription: "管理擷取行為與歷史保留規則。",
    storageSettings: "儲存",
    behaviorSettings: "擷取與行為",
    unpinItem: "取消固定",
    pinned: "已固定",
    pinUpdated: "固定狀態已更新。",
    pinRetentionHint:
      "固定項目不會被清除或自動淘汰；僅固定項目就超出數量或容量上限時，仍會保留。取消固定後立即套用目前上限。",
    clipboardItemCopied: "已複製到剪貼簿。",
    remoteClipboard: "來自其他裝置",
    previewTruncated: "摘要已縮短",
    captureRejected:
      "由於內容超出安全限制或要求保護隱私，一筆剪貼簿內容未被儲存。",
    retry: "重試",
    dismiss: "關閉",
    diagnosticDetails: "安全診斷資訊",
    copyDiagnostic: "複製診斷資訊",
    diagnosticLoading: "正在載入安全診斷資訊...",
    diagnosticUnavailable: "無法取得安全診斷資訊。",
    diagnosticCopied: "診斷資訊已複製。",
    diagnosticCopyFailed: "無法複製診斷資訊。",
    commandError: (operation, code) =>
      code === "restore_post_processing_failed"
        ? "已複製，但歷史記錄或選單重新整理失敗。"
        : traditionalChineseOperationErrors[operation],
    reduceHistory: "減少儲存的歷史記錄？",
    reduceHistoryDescription: (current, next, deleteCount) =>
      `將儲存上限從 ${current} 改為 ${next}，將從最舊的未固定項目開始，最多刪除 ${deleteCount} 筆記錄，固定項目會保留。`,
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
