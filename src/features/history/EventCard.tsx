import {
  AlertTriangle,
  Check,
  Copy,
  File,
  Files,
  Folder,
  Image as ImageIcon,
  Search,
  Pin,
  PinOff,
  Trash2,
  Video,
} from "lucide-react";
import type { ElementRef, KeyboardEvent } from "react";
import type { Messages, SupportedLanguage } from "../../i18n";
import { getEventTypeLabel } from "../../i18n";
import {
  decodeSummaryDisplay,
  getDisplayWidth,
  parseFileDisplay,
  truncateContent,
} from "../../lib/display";
import type { FileDisplayItem } from "../../lib/display";
import type {
  FileDetailItem,
  HistoryDetail,
  HistorySummary,
  RichPreviewSegment,
} from "../../types";
import {
  HtmlPreview,
  ImageThumbnail,
  TextPreview,
  VideoMetadata,
} from "./PreviewMedia";

interface EventCardProps {
  summary: HistorySummary;
  detail: HistoryDetail | undefined;
  detailLoading: boolean;
  detailFailed: boolean;
  expanded: boolean;
  copied: boolean;
  restoring: boolean;
  pinning: boolean;
  deleting?: boolean;
  searchQuery?: string;
  language: SupportedLanguage;
  messages: Messages;
  onToggle: () => void;
  onRetryDetail: () => void;
  onRestore: () => void;
  onDelete: (trigger: ElementRef<"button">) => void;
  onPin: () => void;
}

function renderEventTypeIcon(dataType: string) {
  switch (dataType) {
    case "file":
      return <File aria-hidden="true" className="event-type-icon" size={18} />;
    case "folder":
      return (
        <Folder aria-hidden="true" className="event-type-icon" size={18} />
      );
    case "video":
      return <Video aria-hidden="true" className="event-type-icon" size={18} />;
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
      return <Files aria-hidden="true" className="event-type-icon" size={18} />;
    case "folders":
      return (
        <Folder aria-hidden="true" className="event-type-icon" size={18} />
      );
    default:
      return null;
  }
}

function richTypeLabel(
  messages: Messages,
  segments: readonly RichPreviewSegment[]
): string | null {
  const hasText = segments.some(segment => segment.type === "text");
  const hasImage = segments.some(segment => segment.type === "image");
  const hasVideo = segments.some(segment => segment.type === "video");
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
  return null;
}

function RichSegment({
  index,
  messages,
  segment,
}: {
  index: number;
  messages: Messages;
  segment: RichPreviewSegment;
}) {
  if (segment.type === "text") {
    return (
      <p className="event-text event-rich-text" key={`text-${index}`}>
        {segment.text}
      </p>
    );
  }

  if (segment.type === "video") {
    const label = segment.label === "Video" ? messages.video : segment.label;
    return (
      <VideoMetadata
        key={`video-${index}`}
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
        data={segment.data}
        mediaType={segment.media_type}
      />
    </div>
  );
}

function FileItems({
  expanded,
  items,
  messages,
}: {
  expanded: boolean;
  items: (FileDisplayItem | FileDetailItem)[];
  messages: Messages;
}) {
  const visibleItems = expanded ? items : items.slice(0, 1);
  return (
    <ul className="event-file-items">
      {visibleItems.map((item, index) => {
        const itemLabel =
          item.name.length > 0
            ? item.name
            : item.type === "folder"
              ? messages.folderFallbackName(index + 1)
              : messages.fileFallbackName(index + 1);
        const hiddenItemCount = expanded
          ? 0
          : Math.max(0, items.length - visibleItems.length);
        const collapsedSuffix =
          hiddenItemCount > 0 ? messages.moreItems(hiddenItemCount) : "";
        const label = expanded
          ? itemLabel
          : `${truncateContent(
              itemLabel,
              Math.max(0, 40 - getDisplayWidth(collapsedSuffix))
            )}${collapsedSuffix}`;

        return (
          <li
            className="event-file-item"
            key={`${item.type}-${itemLabel}-${index}`}
          >
            {item.type === "folder" ? (
              <Folder
                aria-hidden="true"
                className="event-type-icon"
                size={18}
              />
            ) : (
              <File aria-hidden="true" className="event-type-icon" size={18} />
            )}
            <span className="event-file-description">
              <span>{label}</span>
              {expanded && "path" in item && item.path && (
                <span className="event-file-path">{item.path}</span>
              )}
            </span>
          </li>
        );
      })}
    </ul>
  );
}

export function EventCard({
  summary,
  detail,
  detailLoading,
  detailFailed,
  expanded,
  copied,
  restoring,
  pinning,
  deleting = false,
  searchQuery = "",
  language,
  messages,
  onToggle,
  onRetryDetail,
  onRestore,
  onDelete,
  onPin,
}: EventCardProps) {
  const fallbackLabel = getEventTypeLabel(messages, summary.data_type);
  const text = decodeSummaryDisplay(summary, fallbackLabel, messages.video);
  const summaryFileItems = parseFileDisplay(text);
  const fileItems =
    expanded && detail?.file_items?.length
      ? detail.file_items
      : summaryFileItems;
  const searchPreview = summary.search_preview?.trim() ?? "";
  const collapsedSearchSurface = fileItems
    ? (fileItems[0]?.name ?? "")
    : truncateContent(text);
  const searchVisibleInCollapsedSummary = collapsedSearchSurface
    .toLocaleLowerCase(language)
    .includes(searchQuery.toLocaleLowerCase(language));
  const showSearchPreview =
    !expanded &&
    searchPreview.length > 0 &&
    searchQuery.length > 0 &&
    !searchVisibleInCollapsedSummary;
  const richSegments = detail?.rich_preview ?? [];
  const typeLabel = expanded
    ? (richTypeLabel(messages, richSegments) ?? fallbackLabel)
    : fallbackLabel;

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.target !== event.currentTarget) return;
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    onToggle();
  };

  return (
    <article
      className={`event-card ${expanded ? "event-card-expanded" : ""} ${
        copied ? "event-card-copied" : ""
      } ${summary.is_pinned ? "event-card-pinned" : ""}`}
      data-history-hash={summary.content_hash}
      onClick={onToggle}
    >
      <div
        aria-expanded={expanded}
        className="event-content"
        onKeyDown={handleKeyDown}
        role="button"
        tabIndex={0}
      >
        <p className="event-meta">
          <span>{typeLabel}</span>
          {summary.is_pinned && (
            <span className="event-pinned-badge">
              <Pin size={12} aria-hidden="true" />
              {messages.pinned}
            </span>
          )}
          {summary.is_remote_clipboard && (
            <span className="event-remote-badge">
              {messages.remoteClipboard}
            </span>
          )}
          {summary.display_truncated && (
            <span className="event-truncated-badge">
              {messages.previewTruncated}
            </span>
          )}
        </p>

        {expanded && detailLoading ? (
          <p className="event-detail-status" role="status">
            {messages.loadingDetail}
          </p>
        ) : expanded && detailFailed ? (
          <div className="event-detail-status" role="alert">
            <span>{messages.detailUnavailable}</span>
            <button
              className="btn btn-secondary event-detail-retry"
              onClick={event => {
                event.stopPropagation();
                onRetryDetail();
              }}
              type="button"
            >
              {messages.retry}
            </button>
          </div>
        ) : expanded && detail?.html_preview ? (
          <HtmlPreview
            html={detail.html_preview}
            title={messages.formattedPreviewTitle}
          />
        ) : expanded && detail?.text_preview ? (
          <TextPreview
            text={detail.text_preview}
            title={messages.formattedPreviewTitle}
          />
        ) : expanded && richSegments.length > 0 ? (
          <div className="event-rich-preview">
            {richSegments.map((segment, index) => (
              <RichSegment
                index={index}
                key={`${segment.type}-${index}`}
                messages={messages}
                segment={segment}
              />
            ))}
          </div>
        ) : fileItems ? (
          <FileItems
            expanded={expanded}
            items={fileItems}
            messages={messages}
          />
        ) : (
          <div className="event-preview">
            {renderEventTypeIcon(summary.data_type)}
            <p className="event-text">
              {expanded ? text : truncateContent(text)}
            </p>
          </div>
        )}

        {showSearchPreview && (
          <p className="event-search-match">
            <Search aria-hidden="true" size={14} strokeWidth={2.2} />
            <span>
              <span className="sr-only">{messages.searchMatch}: </span>
              {searchPreview}
            </span>
          </p>
        )}

        <p className="event-timestamp">
          {new Date(summary.timestamp).toLocaleString(language)}
        </p>
      </div>

      <div className="event-actions">
        <button
          aria-label={summary.is_pinned ? messages.unpinItem : messages.pinItem}
          aria-pressed={summary.is_pinned}
          className={`btn btn-secondary ${summary.is_pinned ? "btn-pinned" : ""}`}
          disabled={pinning || deleting}
          title={summary.is_pinned ? messages.unpinItem : messages.pinItem}
          onClick={event => {
            event.stopPropagation();
            onPin();
          }}
          type="button"
        >
          {summary.is_pinned ? (
            <PinOff size={16} aria-hidden="true" />
          ) : (
            <Pin size={16} aria-hidden="true" />
          )}
        </button>
        <button
          aria-label={
            copied
              ? messages.copiedToClipboard
              : restoring
                ? messages.restoringToClipboard
                : messages.restoreToClipboard
          }
          className={`btn btn-primary copy-feedback-button ${
            copied ? "btn-copy-success" : ""
          }`}
          onClick={event => {
            event.stopPropagation();
            onRestore();
          }}
          disabled={restoring || deleting}
          title={
            copied
              ? messages.copiedToClipboard
              : restoring
                ? messages.restoringToClipboard
                : messages.restoreToClipboard
          }
          type="button"
        >
          {copied ? (
            <Check className="copy-feedback-icon" size={16} />
          ) : (
            <Copy size={16} />
          )}
        </button>
        <button
          aria-label={deleting ? messages.deletingItem : messages.deleteItem}
          className="btn btn-danger"
          disabled={deleting || pinning}
          onClick={event => {
            event.stopPropagation();
            onDelete(event.currentTarget);
          }}
          title={messages.deleteItem}
          type="button"
        >
          <Trash2 size={16} />
        </button>
      </div>
    </article>
  );
}
