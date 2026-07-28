import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AlertTriangle,
  ArrowUpDown,
  Check,
  Copy,
  Eye,
  EyeOff,
  File,
  Files,
  Folder,
  Image as ImageIcon,
  RefreshCw,
  Trash2,
  Type,
  Video,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import "./App.css";
import {
  detectSystemLanguage,
  getEventTypeLabel as getLocalizedEventTypeLabel,
  getMessages,
  isLanguagePreference,
  isSupportedLanguage,
  languageDisplayNames,
  languagePreferences,
} from "./i18n";
import type { LanguagePreference, SupportedLanguage } from "./i18n";

const currentWindowLabel = getCurrentWindow().label;
const isSettingsWindow = currentWindowLabel === "settings";
const fileDisplayFormat = "copy_stack.file-items.v1";
const displayMaxWidth = 40;
const truncationSuffix = "...";
const pngSignature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

interface StoredEvent {
  content_hash: string;
  data_type: string;
  display: number[];
  html_preview: string | null;
  rich_preview: RichPreviewSegment[];
  timestamp: number;
}

interface AppSettings {
  max_items: number;
  show_in_menu_bar: boolean;
  move_restored_item_to_top: boolean;
  compact_mode: boolean;
  language: string;
  resolved_language: string;
}

interface FileDisplayItem {
  type: string;
  name: string;
}

interface FileDisplayPayload {
  format: string;
  items: FileDisplayItem[];
}

interface DisplayPreview {
  text: string;
  fileItems: FileDisplayItem[] | null;
  html: string | null;
  image: ImageDisplay | null;
  richSegments: RichPreviewSegment[];
}

interface ImageDisplay {
  bytes: Uint8Array;
  mediaType: string;
  label: string;
}

type RichPreviewSegment =
  | RichPreviewTextSegment
  | RichPreviewImageSegment
  | RichPreviewVideoSegment;

interface RichPreviewTextSegment {
  type: "text";
  text: string;
}

interface RichPreviewImageSegment {
  type: "image";
  label: string;
  media_type: string;
  data: number[];
}

interface RichPreviewVideoSegment {
  type: "video";
  label: string;
  media_type: string;
  path: string;
}

const blockedHtmlPreviewElements =
  "script, noscript, iframe, frame, frameset, object, embed, form, input, button, textarea, select, option, link, base, meta";
const blockedHtmlPreviewUrlAttributes = new Set([
  "action",
  "formaction",
  "href",
  "poster",
  "src",
  "srcdoc",
  "srcset",
  "xlink:href",
]);

function buildHtmlPreviewDocument(html: string) {
  const parsed = new window.DOMParser().parseFromString(html, "text/html");
  parsed
    .querySelectorAll(blockedHtmlPreviewElements)
    .forEach(element => element.remove());

  parsed.querySelectorAll("*").forEach(element => {
    Array.from(element.attributes).forEach(attribute => {
      const attributeName = attribute.name.toLowerCase();
      const isInlineHandler = attributeName.startsWith("on");
      const isDataImage =
        element.tagName === "IMG" &&
        attributeName === "src" &&
        /^data:image\/(?:bmp|gif|jpeg|jpg|png|webp);/i.test(attribute.value);

      if (
        isInlineHandler ||
        (blockedHtmlPreviewUrlAttributes.has(attributeName) && !isDataImage)
      ) {
        element.removeAttribute(attribute.name);
      }
    });
  });

  const preservedStyles = Array.from(parsed.head.querySelectorAll("style")).map(
    style => style.outerHTML
  );
  const previewStyles = `
    :root { color-scheme: light; background: #fff; }
    * { box-sizing: border-box; max-width: 100%; }
    body {
      margin: 0;
      padding: 16px;
      overflow-wrap: anywhere;
      color: #14213d;
      background: #fff;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      line-height: 1.55;
    }
    img { height: auto; }
    table { border-collapse: collapse; }
  `;

  return `<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:">
    <style>${previewStyles}</style>
    ${preservedStyles.join("\n")}
  </head>
  <body>${parsed.body.innerHTML}</body>
</html>`;
}

function HtmlPreview({ html, title }: { html: string; title: string }) {
  return (
    <div
      className="event-html-preview-shell"
      onClick={event => event.stopPropagation()}
    >
      <iframe
        className="event-html-preview"
        referrerPolicy="no-referrer"
        sandbox=""
        srcDoc={buildHtmlPreviewDocument(html)}
        title={title}
      />
    </div>
  );
}

function ImageThumbnail({
  alt,
  bytes,
  mediaType,
}: ImageDisplay & { alt: string }) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);

  useEffect(() => {
    const blob = new window.Blob([bytes], { type: mediaType });
    const nextImageUrl = window.URL.createObjectURL(blob);
    setImageUrl(nextImageUrl);

    return () => {
      window.URL.revokeObjectURL(nextImageUrl);
    };
  }, [bytes, mediaType]);

  if (!imageUrl) {
    return <div aria-hidden="true" className="event-image-placeholder" />;
  }

  return (
    <img
      alt={alt}
      className="event-image-thumbnail"
      draggable={false}
      src={imageUrl}
    />
  );
}

function VideoThumbnail({
  coverAlt,
  label,
  path,
}: RichPreviewVideoSegment & { coverAlt: string }) {
  const [coverUrl, setCoverUrl] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const video = window.document.createElement("video");

    const captureFrame = () => {
      if (cancelled || video.videoWidth === 0 || video.videoHeight === 0) {
        return;
      }

      try {
        const canvas = window.document.createElement("canvas");
        canvas.width = video.videoWidth;
        canvas.height = video.videoHeight;
        const context = canvas.getContext("2d");
        if (!context) {
          return;
        }

        context.drawImage(video, 0, 0, canvas.width, canvas.height);
        setCoverUrl(canvas.toDataURL("image/jpeg", 0.82));
      } catch {
        setCoverUrl(null);
      }
    };

    const seekToCoverFrame = () => {
      if (!Number.isFinite(video.duration) || video.duration <= 0.05) {
        captureFrame();
        return;
      }

      video.currentTime = Math.min(1, video.duration / 4);
    };

    video.muted = true;
    video.playsInline = true;
    video.preload = "auto";
    video.addEventListener("loadedmetadata", seekToCoverFrame);
    video.addEventListener("seeked", captureFrame);
    video.addEventListener("loadeddata", captureFrame, { once: true });
    video.addEventListener("error", () => setCoverUrl(null), { once: true });
    video.src = `${convertFileSrc(path)}#t=1`;
    video.load();

    return () => {
      cancelled = true;
      video.removeEventListener("loadedmetadata", seekToCoverFrame);
      video.removeEventListener("seeked", captureFrame);
      video.removeAttribute("src");
      video.load();
    };
  }, [path]);

  return (
    <div
      className="event-video-preview"
      onClick={event => event.stopPropagation()}
    >
      <div className="event-video-frame">
        {coverUrl ? (
          <img
            alt={coverAlt}
            className="event-video-cover"
            draggable={false}
            src={coverUrl}
          />
        ) : (
          <div
            aria-label={coverAlt}
            className="event-video-cover event-video-cover-placeholder"
          />
        )}
        <span className="event-video-icon" aria-hidden="true">
          <Video size={22} />
        </span>
      </div>
      <p className="event-text event-video-label">{label}</p>
    </div>
  );
}

function App() {
  const [copyEvents, setCopyEvents] = useState<StoredEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [settingsLoading, setSettingsLoading] = useState(false);
  const [maxItems, setMaxItems] = useState(100);
  const [pendingMaxItemsInput, setPendingMaxItemsInput] = useState("100");
  const [menuBarVisible, setMenuBarVisible] = useState(true);
  const [moveRestoredItemToTop, setMoveRestoredItemToTop] = useState(false);
  const [compactMode, setCompactMode] = useState(false);
  const [languagePreference, setLanguagePreference] =
    useState<LanguagePreference>("system");
  const [language, setLanguage] = useState<SupportedLanguage>(() =>
    detectSystemLanguage()
  );
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const [eventsToDelete, setEventsToDelete] = useState(0);
  const [copiedEventHash, setCopiedEventHash] = useState<string | null>(null);
  const copiedFeedbackTimerRef = useRef<number | null>(null);
  const [expandedEventHashes, setExpandedEventHashes] = useState<Set<string>>(
    () => new Set()
  );
  const messages = getMessages(language);

  useEffect(() => {
    return () => {
      if (copiedFeedbackTimerRef.current !== null) {
        window.clearTimeout(copiedFeedbackTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    document.documentElement.lang = language;
    document.title = isSettingsWindow ? messages.settings : "Copy Stack";
  }, [language, messages.settings]);

  const loadEvents = useCallback(async () => {
    setRefreshing(true);
    try {
      const events = await invoke<StoredEvent[]>("get_copy_events");
      setCopyEvents(events);
      setExpandedEventHashes(current => {
        const currentHashes = new Set(events.map(event => event.content_hash));
        const next = new Set(
          Array.from(current).filter(contentHash =>
            currentHashes.has(contentHash)
          )
        );
        return next.size === current.size ? current : next;
      });
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to load clipboard history", error);
      }
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  const applySettings = useCallback(
    (settings: AppSettings, preservePendingMaxItems = false) => {
      setMaxItems(settings.max_items);
      if (!preservePendingMaxItems) {
        setPendingMaxItemsInput(String(settings.max_items));
      }
      setMenuBarVisible(settings.show_in_menu_bar);
      setMoveRestoredItemToTop(settings.move_restored_item_to_top);
      setCompactMode(settings.compact_mode);
      setLanguagePreference(
        isLanguagePreference(settings.language) ? settings.language : "system"
      );
      setLanguage(
        isSupportedLanguage(settings.resolved_language)
          ? settings.resolved_language
          : detectSystemLanguage()
      );
    },
    []
  );

  const loadSettings = useCallback(
    async (preservePendingMaxItems = false) => {
      try {
        const settings = await invoke<AppSettings>("get_app_settings");
        applySettings(settings, preservePendingMaxItems);
      } catch (error) {
        if (import.meta.env.DEV) {
          console.error("Failed to load app settings", error);
        }
      }
    },
    [applySettings]
  );

  useEffect(() => {
    void loadEvents();
    void loadSettings();

    let disposed = false;
    let unlisteners: (() => void)[] = [];

    const registerListeners = async () => {
      const registered: (() => void)[] = [];
      try {
        registered.push(
          await listen("clipboard-history-updated", () => {
            void loadEvents();
            void loadSettings(true);
          })
        );
        if (disposed) {
          registered.forEach(unlisten => unlisten());
          return;
        }

        registered.push(
          await listen("app-language-changed", () => {
            void loadSettings(true);
          })
        );
        if (disposed) {
          registered.forEach(unlisten => unlisten());
          return;
        }

        unlisteners = registered;
      } catch (error) {
        registered.forEach(unlisten => unlisten());
        throw error;
      }
    };

    void registerListeners().catch(error => {
      if (import.meta.env.DEV) {
        console.error("Failed to register app event listeners", error);
      }
    });

    return () => {
      disposed = true;
      unlisteners.forEach(unlisten => unlisten());
    };
  }, [loadEvents, loadSettings]);

  const parsedPendingMaxItems = Number.parseInt(pendingMaxItemsInput, 10);
  const isPendingMaxItemsValid =
    Number.isInteger(parsedPendingMaxItems) &&
    parsedPendingMaxItems >= 1 &&
    parsedPendingMaxItems <= 1000;
  const isStorageLimitDirty =
    isPendingMaxItemsValid && parsedPendingMaxItems !== maxItems;

  const updateMaxItems = async (newMaxItems: number) => {
    setSettingsLoading(true);
    try {
      await invoke("set_max_items", { maxItems: newMaxItems });
      setMaxItems(newMaxItems);
      setPendingMaxItemsInput(String(newMaxItems));
      await loadEvents();
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to update max items", error);
      }
    } finally {
      setSettingsLoading(false);
    }
  };

  const updateMenuBarVisibility = async (nextVisible: boolean) => {
    setSettingsLoading(true);
    try {
      await invoke("set_show_in_menu_bar", {
        showInMenuBar: nextVisible,
      });
      setMenuBarVisible(nextVisible);
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to update menu bar visibility", error);
      }
    } finally {
      setSettingsLoading(false);
    }
  };

  const updateRestoreOrdering = async (nextEnabled: boolean) => {
    setSettingsLoading(true);
    try {
      await invoke("set_move_restored_item_to_top", {
        moveRestoredItemToTop: nextEnabled,
      });
      setMoveRestoredItemToTop(nextEnabled);
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to update restore ordering", error);
      }
    } finally {
      setSettingsLoading(false);
    }
  };

  const updateCompactMode = async (nextEnabled: boolean) => {
    setSettingsLoading(true);
    try {
      await invoke("set_compact_mode", {
        compactMode: nextEnabled,
      });
      setCompactMode(nextEnabled);
      await loadEvents();
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to update compact mode", error);
      }
    } finally {
      setSettingsLoading(false);
    }
  };

  const updateLanguage = async (nextLanguage: LanguagePreference) => {
    setSettingsLoading(true);
    try {
      const settings = await invoke<AppSettings>("set_language", {
        language: nextLanguage,
      });
      applySettings(settings, true);
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to update language", error);
      }
    } finally {
      setSettingsLoading(false);
    }
  };

  const handleApplyStorageLimit = async () => {
    if (!isPendingMaxItemsValid || !isStorageLimitDirty) {
      return;
    }

    if (parsedPendingMaxItems < copyEvents.length) {
      setEventsToDelete(copyEvents.length - parsedPendingMaxItems);
      setShowConfirmDialog(true);
      return;
    }

    await updateMaxItems(parsedPendingMaxItems);
  };

  const confirmMaxItemsChange = async () => {
    setShowConfirmDialog(false);
    if (!isPendingMaxItemsValid) {
      return;
    }
    await updateMaxItems(parsedPendingMaxItems);
  };

  const cancelMaxItemsChange = () => {
    setShowConfirmDialog(false);
    setPendingMaxItemsInput(String(maxItems));
  };

  const deleteEvent = async (contentHash: string) => {
    try {
      await invoke("delete_copy_event", { contentHash });
      await loadEvents();
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to delete clipboard item", error);
      }
    }
  };

  const copyToClipboard = async (contentHash: string) => {
    if (import.meta.env.DEV) {
      console.info("[copy_stack] restore requested from UI", { contentHash });
    }
    try {
      await invoke("copy_to_clipboard", { contentHash });
      setCopiedEventHash(contentHash);
      if (copiedFeedbackTimerRef.current !== null) {
        window.clearTimeout(copiedFeedbackTimerRef.current);
      }
      copiedFeedbackTimerRef.current = window.setTimeout(() => {
        setCopiedEventHash(null);
        copiedFeedbackTimerRef.current = null;
      }, 1400);
      if (import.meta.env.DEV) {
        console.info("[copy_stack] restore command completed", { contentHash });
      }
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("[copy_stack] failed to restore clipboard item", {
          contentHash,
          error,
        });
      }
    }
  };

  const clearAllEvents = async () => {
    try {
      await invoke("clear_all_events");
      await loadEvents();
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to clear clipboard history", error);
      }
    }
  };

  const toggleEventExpansion = (contentHash: string) => {
    setExpandedEventHashes(current => {
      const next = new Set(current);
      if (next.has(contentHash)) {
        next.delete(contentHash);
      } else {
        next.add(contentHash);
      }
      return next;
    });
  };

  const handleEventCardKeyDown = (
    event: KeyboardEvent<HTMLElement>,
    contentHash: string
  ) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }

    event.preventDefault();
    toggleEventExpansion(contentHash);
  };

  const formatTimestamp = (timestamp: number) => {
    return new Date(timestamp).toLocaleString(language);
  };

  const getCharacterDisplayWidth = (character: string) => {
    if (
      /[\u1100-\u115F\u2329\u232A\u2E80-\uA4CF\uAC00-\uD7A3\uF900-\uFAFF\uFE10-\uFE19\uFE30-\uFE6F\uFF00-\uFF60\uFFE0-\uFFE6\u{1F300}-\u{1FAFF}]/u.test(
        character
      )
    ) {
      return 2;
    }

    return 1;
  };

  const getDisplayWidth = (content: string) => {
    return Array.from(content).reduce(
      (width, character) => width + getCharacterDisplayWidth(character),
      0
    );
  };

  const truncateContent = (content: string, maxWidth = displayMaxWidth) => {
    const flattened = content.replace(/\s+/g, " ").trim();
    if (getDisplayWidth(flattened) <= maxWidth) {
      return flattened;
    }

    const suffixWidth = getDisplayWidth(truncationSuffix);
    const availableWidth = Math.max(0, maxWidth - suffixWidth);
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
  };

  const decodeDisplayText = (event: StoredEvent) => {
    const text = new TextDecoder().decode(new Uint8Array(event.display));
    if (text.includes("\uFFFD")) {
      return getLocalizedEventTypeLabel(messages, event.data_type);
    }
    if (event.data_type === "video" && text === "Video") {
      return messages.video;
    }
    return text;
  };

  const parseFileDisplay = (text: string) => {
    try {
      const parsed = JSON.parse(text) as Partial<FileDisplayPayload>;
      if (parsed.format !== fileDisplayFormat || !Array.isArray(parsed.items)) {
        return null;
      }

      const items = parsed.items.filter((item): item is FileDisplayItem => {
        const candidate = item as Partial<FileDisplayItem> | null;
        return (
          typeof candidate?.type === "string" &&
          typeof candidate?.name === "string"
        );
      });
      return items.length > 0 ? items : null;
    } catch {
      return null;
    }
  };

  const isPngDisplay = (display: number[]) => {
    return pngSignature.every((byte, index) => display[index] === byte);
  };

  const parseImageDisplay = (event: StoredEvent): ImageDisplay | null => {
    if (event.data_type !== "png" || !isPngDisplay(event.display)) {
      return null;
    }

    return {
      bytes: new Uint8Array(event.display),
      mediaType: "image/png",
      label: messages.pngImage,
    };
  };

  const getDisplayPreview = (event: StoredEvent): DisplayPreview => {
    const image = parseImageDisplay(event);
    const text = decodeDisplayText(event);
    const fileItems = parseFileDisplay(text);
    return {
      text: image ? image.label : text,
      fileItems,
      html: typeof event.html_preview === "string" ? event.html_preview : null,
      image,
      richSegments: Array.isArray(event.rich_preview) ? event.rich_preview : [],
    };
  };

  const getEventTypeLabel = (event: StoredEvent, preview: DisplayPreview) => {
    if (preview.richSegments.length > 0) {
      const hasText = preview.richSegments.some(
        segment => segment.type === "text"
      );
      const hasImage = preview.richSegments.some(
        segment => segment.type === "image"
      );
      const hasVideo = preview.richSegments.some(
        segment => segment.type === "video"
      );

      if (hasText && hasImage) {
        return messages.textAndImage;
      }
      if (hasVideo) {
        return messages.video;
      }
      if (hasImage) {
        return messages.image;
      }
      if (hasText) {
        return messages.text;
      }
    }

    return getLocalizedEventTypeLabel(messages, event.data_type);
  };

  const renderRichPreviewSegment = (
    segment: RichPreviewSegment,
    index: number,
    isExpanded: boolean
  ) => {
    if (segment.type === "text") {
      return (
        <p className="event-text event-rich-text" key={`text-${index}`}>
          {isExpanded ? segment.text : truncateContent(segment.text, 96)}
        </p>
      );
    }

    if (segment.type === "video") {
      const label = segment.label === "Video" ? messages.video : segment.label;
      return (
        <VideoThumbnail
          key={`video-${index}`}
          {...segment}
          coverAlt={messages.videoCoverAlt(label)}
          label={label}
        />
      );
    }

    const label = segment.label === "Image" ? messages.image : segment.label;
    return (
      <div className="event-rich-image" key={`image-${index}`}>
        <ImageThumbnail
          alt={messages.imageThumbnailAlt(label)}
          bytes={new Uint8Array(segment.data)}
          label={label}
          mediaType={segment.media_type}
        />
      </div>
    );
  };

  const renderFileItemIcon = (itemType: string) => {
    if (itemType === "folder") {
      return (
        <Folder aria-hidden="true" className="event-type-icon" size={18} />
      );
    }

    return <File aria-hidden="true" className="event-type-icon" size={18} />;
  };

  const getFileItemLabel = (item: FileDisplayItem, index: number) => {
    if (item.name.length > 0) {
      return item.name;
    }

    return item.type === "folder"
      ? messages.folderFallbackName(index + 1)
      : messages.fileFallbackName(index + 1);
  };

  const renderEventTypeIcon = (dataType: string) => {
    switch (dataType) {
      case "file":
        return (
          <File aria-hidden="true" className="event-type-icon" size={18} />
        );
      case "folder":
        return (
          <Folder aria-hidden="true" className="event-type-icon" size={18} />
        );
      case "video":
        return (
          <Video aria-hidden="true" className="event-type-icon" size={18} />
        );
      case "unsupported":
        return (
          <AlertTriangle
            aria-hidden="true"
            className="event-type-icon"
            size={18}
          />
        );
      case "png":
      case "jpg":
      case "jpeg":
      case "gif":
      case "webp":
      case "tiff":
      case "tif":
      case "bmp":
      case "heic":
      case "heif":
        return (
          <ImageIcon aria-hidden="true" className="event-type-icon" size={18} />
        );
      case "files":
      case "files and folders":
        return (
          <Files aria-hidden="true" className="event-type-icon" size={18} />
        );
      case "folders":
        return (
          <Folder aria-hidden="true" className="event-type-icon" size={18} />
        );
      default:
        return null;
    }
  };

  return (
    <div className={`app-shell ${isSettingsWindow ? "settings-shell" : ""}`}>
      {isSettingsWindow ? (
        <main className="preferences-panel">
          <header className="preferences-header">
            <h1>{messages.settings}</h1>
          </header>

          <section className="preference-group">
            <div className="preference-row">
              <span className="preference-copy">
                <label htmlFor="language-select">{messages.language}</label>
                <span className="preference-description">
                  {languagePreference === "system"
                    ? messages.languageDescriptionSystem(
                        languageDisplayNames[language]
                      )
                    : messages.languageDescriptionManual}
                </span>
              </span>
              <select
                id="language-select"
                className="language-select"
                value={languagePreference}
                onChange={event => {
                  if (isLanguagePreference(event.target.value)) {
                    void updateLanguage(event.target.value);
                  }
                }}
                disabled={settingsLoading}
              >
                {languagePreferences.map(preference => (
                  <option key={preference} value={preference}>
                    {preference === "system"
                      ? messages.systemDefault
                      : languageDisplayNames[preference]}
                  </option>
                ))}
              </select>
            </div>

            <div className="preference-row preference-row-stacked">
              <div className="preference-copy">
                <label htmlFor="max-items-input">{messages.storedItems}</label>
                <p>
                  {messages.storedItemsDescription(maxItems, copyEvents.length)}
                </p>
              </div>
              <div className="preference-control storage-input-row">
                <input
                  id="max-items-input"
                  type="number"
                  min="1"
                  max="1000"
                  value={pendingMaxItemsInput}
                  onChange={event =>
                    setPendingMaxItemsInput(event.target.value)
                  }
                  disabled={settingsLoading}
                  className="storage-input"
                />
                <button
                  className="btn btn-primary"
                  onClick={() => void handleApplyStorageLimit()}
                  disabled={
                    settingsLoading ||
                    !isPendingMaxItemsValid ||
                    !isStorageLimitDirty
                  }
                >
                  {messages.apply}
                </button>
              </div>
              {!isPendingMaxItemsValid && (
                <p className="settings-error">{messages.storageLimitError}</p>
              )}
            </div>

            <label className="preference-row">
              <span className="preference-copy">
                <span className="preference-title">{messages.compactMode}</span>
                <span className="preference-description">
                  <Type size={13} />
                  {compactMode
                    ? messages.compactModeEnabled
                    : messages.compactModeDisabled}
                </span>
              </span>
              <span className="mac-switch">
                <input
                  type="checkbox"
                  checked={compactMode}
                  onChange={event =>
                    void updateCompactMode(event.target.checked)
                  }
                  disabled={settingsLoading}
                />
                <span className="mac-switch-track" />
              </span>
            </label>

            <label className="preference-row">
              <span className="preference-copy">
                <span className="preference-title">
                  {messages.moveRestoredItemsToTop}
                </span>
                <span className="preference-description">
                  <ArrowUpDown size={13} />
                  {moveRestoredItemToTop
                    ? messages.restoreOrderingEnabled
                    : messages.restoreOrderingDisabled}
                </span>
              </span>
              <span className="mac-switch">
                <input
                  type="checkbox"
                  checked={moveRestoredItemToTop}
                  onChange={event =>
                    void updateRestoreOrdering(event.target.checked)
                  }
                  disabled={settingsLoading}
                />
                <span className="mac-switch-track" />
              </span>
            </label>

            <label className="preference-row">
              <span className="preference-copy">
                <span className="preference-title">
                  {messages.showInMenuBar}
                </span>
                <span className="preference-description">
                  {menuBarVisible ? <Eye size={13} /> : <EyeOff size={13} />}
                  {menuBarVisible
                    ? messages.menuBarEnabled
                    : messages.menuBarDisabled}
                </span>
              </span>
              <span className="mac-switch">
                <input
                  type="checkbox"
                  checked={menuBarVisible}
                  onChange={event =>
                    void updateMenuBarVisibility(event.target.checked)
                  }
                  disabled={settingsLoading}
                />
                <span className="mac-switch-track" />
              </span>
            </label>
          </section>
        </main>
      ) : (
        <div className="workspace">
          <main className="content-panel">
            <section className="panel-header">
              <div>
                <p className="section-kicker">{messages.clipboardHistory}</p>
                <h2>{messages.recentEvents}</h2>
                <p className="section-description">
                  {messages.historyDescription}
                </p>
              </div>

              <div className="panel-actions">
                <button
                  onClick={() => void loadEvents()}
                  disabled={refreshing}
                  className="btn btn-secondary"
                >
                  <RefreshCw size={16} />
                  {messages.refresh}
                </button>
                <button
                  onClick={() => void clearAllEvents()}
                  className="btn btn-danger"
                  disabled={copyEvents.length === 0}
                >
                  <Trash2 size={16} />
                  {messages.clearAll}
                </button>
              </div>
            </section>

            {loading ? (
              <div className="placeholder-card">{messages.loadingHistory}</div>
            ) : copyEvents.length === 0 ? (
              <div className="empty-state">
                <h3>{messages.emptyHistory}</h3>
                <p>
                  {compactMode
                    ? messages.emptyHistoryCompact
                    : messages.emptyHistoryAll}
                </p>
              </div>
            ) : (
              <div className="events-list">
                {copyEvents.map(event => {
                  const preview = getDisplayPreview(event);
                  const isExpanded = expandedEventHashes.has(
                    event.content_hash
                  );
                  const wasJustCopied = copiedEventHash === event.content_hash;
                  const visibleFileItems =
                    preview.fileItems && !isExpanded
                      ? preview.fileItems.slice(0, 1)
                      : preview.fileItems;
                  return (
                    <article
                      key={event.content_hash}
                      className={`event-card ${
                        isExpanded ? "event-card-expanded" : ""
                      } ${wasJustCopied ? "event-card-copied" : ""}`}
                      role="button"
                      tabIndex={0}
                      aria-expanded={isExpanded}
                      onClick={() => toggleEventExpansion(event.content_hash)}
                      onKeyDown={keyboardEvent =>
                        handleEventCardKeyDown(
                          keyboardEvent,
                          event.content_hash
                        )
                      }
                    >
                      <div className="event-content">
                        <p className="event-meta">
                          <span>{getEventTypeLabel(event, preview)}</span>
                        </p>
                        {isExpanded && preview.html ? (
                          <HtmlPreview
                            html={preview.html}
                            title={messages.formattedPreviewTitle}
                          />
                        ) : preview.richSegments.length > 0 ? (
                          <div className="event-rich-preview">
                            {preview.richSegments.map((segment, index) =>
                              renderRichPreviewSegment(
                                segment,
                                index,
                                isExpanded
                              )
                            )}
                          </div>
                        ) : preview.image ? (
                          <div className="event-image-preview">
                            <ImageThumbnail
                              {...preview.image}
                              alt={messages.imageThumbnailAlt(
                                preview.image.label
                              )}
                            />
                            <p className="event-text">{preview.image.label}</p>
                          </div>
                        ) : visibleFileItems ? (
                          <ul className="event-file-items">
                            {visibleFileItems.map((item, index) => {
                              const itemLabel = getFileItemLabel(item, index);
                              const hiddenItemCount =
                                preview.fileItems && !isExpanded
                                  ? preview.fileItems.length -
                                    visibleFileItems.length
                                  : 0;
                              const collapsedSuffix =
                                hiddenItemCount > 0
                                  ? messages.moreItems(hiddenItemCount)
                                  : "";
                              const collapsedFileLabel =
                                hiddenItemCount > 0
                                  ? `${truncateContent(
                                      itemLabel,
                                      Math.max(
                                        0,
                                        displayMaxWidth -
                                          getDisplayWidth(collapsedSuffix)
                                      )
                                    )}${collapsedSuffix}`
                                  : truncateContent(itemLabel);

                              return (
                                <li
                                  className="event-file-item"
                                  key={`${item.type}-${itemLabel}-${index}`}
                                >
                                  {renderFileItemIcon(item.type)}
                                  <span>
                                    {isExpanded
                                      ? itemLabel
                                      : collapsedFileLabel}
                                  </span>
                                </li>
                              );
                            })}
                          </ul>
                        ) : (
                          <div className="event-preview">
                            {renderEventTypeIcon(event.data_type)}
                            <p className="event-text">
                              {isExpanded
                                ? preview.text
                                : truncateContent(preview.text)}
                            </p>
                          </div>
                        )}
                        <p className="event-timestamp">
                          {formatTimestamp(event.timestamp)}
                        </p>
                      </div>

                      <div className="event-actions">
                        <button
                          onClick={clickEvent => {
                            clickEvent.stopPropagation();
                            void copyToClipboard(event.content_hash);
                          }}
                          aria-label={
                            wasJustCopied
                              ? messages.copiedToClipboard
                              : messages.restoreToClipboard
                          }
                          className={`btn btn-primary copy-feedback-button ${
                            wasJustCopied ? "btn-copy-success" : ""
                          }`}
                          title={
                            wasJustCopied
                              ? messages.copiedToClipboard
                              : messages.restoreToClipboard
                          }
                        >
                          {wasJustCopied ? (
                            <Check className="copy-feedback-icon" size={16} />
                          ) : (
                            <Copy size={16} />
                          )}
                        </button>
                        <button
                          onClick={clickEvent => {
                            clickEvent.stopPropagation();
                            void deleteEvent(event.content_hash);
                          }}
                          className="btn btn-danger"
                          aria-label={messages.deleteItem}
                          title={messages.deleteItem}
                        >
                          <Trash2 size={16} />
                        </button>
                      </div>
                    </article>
                  );
                })}
              </div>
            )}
            <span aria-live="polite" className="sr-only">
              {copiedEventHash ? messages.clipboardItemCopied : ""}
            </span>
          </main>
        </div>
      )}

      {showConfirmDialog && (
        <div className="modal-overlay">
          <div className="modal-content">
            <div className="modal-header">
              <AlertTriangle size={24} className="warning-icon" />
              <h3>{messages.reduceHistory}</h3>
            </div>

            <div className="modal-body">
              <p>
                {messages.reduceHistoryDescription(
                  maxItems,
                  parsedPendingMaxItems,
                  eventsToDelete
                )}
              </p>
              <p className="warning-text">{messages.cannotUndo}</p>
            </div>

            <div className="modal-actions">
              <button
                onClick={cancelMaxItemsChange}
                className="btn btn-secondary"
                disabled={settingsLoading}
              >
                {messages.cancel}
              </button>
              <button
                onClick={() => void confirmMaxItemsChange()}
                className="btn btn-danger"
                disabled={settingsLoading}
              >
                {settingsLoading ? messages.updating : messages.deleteAndUpdate}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
