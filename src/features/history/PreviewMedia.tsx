import { Video } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { buildHtmlPreview } from "../../lib/htmlPreview";
import { isSafePreviewImage } from "../../lib/display";

export function HtmlPreview({ html, title }: { html: string; title: string }) {
  const preview = useMemo(() => buildHtmlPreview(html), [html]);

  return (
    <div
      className="event-html-preview-shell"
      onClick={event => event.stopPropagation()}
    >
      <iframe
        aria-label={title}
        className={`event-html-preview ${
          preview.compact ? "event-html-preview-compact" : ""
        }`}
        referrerPolicy="no-referrer"
        sandbox=""
        srcDoc={preview.srcDoc}
        tabIndex={-1}
      />
    </div>
  );
}

export function TextPreview({ text, title }: { text: string; title: string }) {
  return (
    <div
      aria-label={title}
      className="event-text-preview-shell"
      onClick={event => event.stopPropagation()}
      role="region"
    >
      <pre className="event-text-preview">{text}</pre>
    </div>
  );
}

export function ImageThumbnail({
  alt,
  data,
  mediaType,
}: {
  alt: string;
  data: readonly number[];
  mediaType: string;
}) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const safe = isSafePreviewImage(data, mediaType);

  useEffect(() => {
    if (!safe) {
      setImageUrl(null);
      return;
    }

    const blob = new window.Blob([new Uint8Array(data)], { type: mediaType });
    const nextImageUrl = window.URL.createObjectURL(blob);
    setImageUrl(nextImageUrl);

    return () => {
      window.URL.revokeObjectURL(nextImageUrl);
    };
  }, [data, mediaType, safe]);

  if (!imageUrl) {
    return <div aria-hidden="true" className="event-image-placeholder" />;
  }

  return (
    <img
      alt={alt}
      className="event-image-thumbnail"
      decoding="async"
      draggable={false}
      loading="lazy"
      src={imageUrl}
    />
  );
}

export function VideoMetadata({
  coverAlt,
  label,
}: {
  coverAlt: string;
  label: string;
}) {
  return (
    <div
      aria-label={coverAlt}
      className="event-video-preview event-video-metadata"
      onClick={event => event.stopPropagation()}
    >
      <span className="event-video-metadata-icon" aria-hidden="true">
        <Video size={22} />
      </span>
      <p className="event-text event-video-label">{label}</p>
    </div>
  );
}
