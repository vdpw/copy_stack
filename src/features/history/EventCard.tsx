import {
  AlertTriangle,
  Check,
  Copy,
  File,
  Files,
  Folder,
  Image as ImageIcon,
  Trash2,
  Video,
} from "lucide-react";
import type { KeyboardEvent } from "react";
import type { Messages, SupportedLanguage } from "../../i18n";
import { getEventTypeLabel } from "../../i18n";
import {
  decodeSummaryDisplay,
  getDisplayWidth,
  parseFileDisplay,
  sourceDisplayName,
  truncateContent,
} from "../../lib/display";
import type { FileDisplayItem } from "../../lib/display";
import type {
  HistoryDetail,
  HistorySummary,
  RichPreviewSegment,
} from "../../types";
import { HtmlPreview, ImageThumbnail, VideoMetadata } from "./PreviewMedia";

interface EventCardProps {
  summary: HistorySummary;
  detail: HistoryDetail | undefined;
  detailLoading: boolean;
  detailFailed: boolean;
  expanded: boolean;
  copied: boolean;
  restoring: boolean;
  language: SupportedLanguage;
  messages: Messages;
  onToggle: () => void;
  onRetryDetail: () => void;
  onRestore: () => void;
  onDelete: () => void;
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
  items: FileDisplayItem[];
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
            <span>{label}</span>
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
  language,
  messages,
  onToggle,
  onRetryDetail,
  onRestore,
  onDelete,
}: EventCardProps) {
  const fallbackLabel = getEventTypeLabel(messages, summary.data_type);
  const text = decodeSummaryDisplay(summary, fallbackLabel, messages.video);
  const fileItems = parseFileDisplay(text);
  const richSegments = detail?.rich_preview ?? [];
  const typeLabel =
    (expanded && richTypeLabel(messages, richSegments)) ?? fallbackLabel;

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    onToggle();
  };

  return (
    <article
      aria-expanded={expanded}
      className={`event-card ${expanded ? "event-card-expanded" : ""} ${
        copied ? "event-card-copied" : ""
      }`}
      data-history-hash={summary.content_hash}
      onClick={onToggle}
      onKeyDown={handleKeyDown}
      role="button"
      tabIndex={0}
    >
      <div className="event-content">
        <p className="event-meta">
          <span>{typeLabel}</span>
          {summary.source_bundle_id !== null &&
            summary.source_bundle_id !== undefined && (
              <span
                className="event-source-badge"
                title={summary.source_bundle_id || messages.unknownSource}
              >
                {messages.sourceBadge(
                  summary.source_bundle_id
                    ? sourceDisplayName(summary.source_bundle_id)
                    : messages.unknownSource
                )}
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

        <p className="event-timestamp">
          {new Date(summary.timestamp).toLocaleString(language)}
        </p>
      </div>

      <div className="event-actions">
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
          disabled={restoring}
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
          aria-label={messages.deleteItem}
          className="btn btn-danger"
          onClick={event => {
            event.stopPropagation();
            onDelete();
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
