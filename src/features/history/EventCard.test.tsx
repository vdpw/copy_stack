// @vitest-environment jsdom

import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { getMessages } from "../../i18n";
import type { HistoryDetail, HistorySummary } from "../../types";
import { EventCard } from "./EventCard";

const textSummary: HistorySummary = {
  content_hash: "text-event",
  data_type: "text",
  display: [101, 120, 97, 109, 112, 108, 101],
  display_truncated: false,
  source_bundle_id: null,
  is_remote_clipboard: false,
  timestamp: 0,
  byte_count: 7,
  has_detail: false,
};

const htmlDetail: HistoryDetail = {
  content_hash: textSummary.content_hash,
  html_preview: "<p><strong>Formatted</strong> preview</p>",
  rich_preview: [],
};

function renderCard(
  expanded: boolean,
  detail: HistoryDetail | undefined = undefined,
  onToggle = vi.fn()
): string {
  return renderToStaticMarkup(
    <EventCard
      copied={false}
      detail={detail}
      detailFailed={false}
      detailLoading={false}
      expanded={expanded}
      language="zh-CN"
      messages={getMessages("zh-CN")}
      onDelete={vi.fn()}
      onRestore={vi.fn()}
      onRetryDetail={vi.fn()}
      onToggle={onToggle}
      restoring={false}
      summary={textSummary}
    />
  );
}

describe("EventCard", () => {
  it("shows the event type label when collapsed or expanded", () => {
    expect(renderCard(false)).toContain(">文字</span>");
    expect(renderCard(true)).toContain(">文字</span>");
  });

  it("keeps formatted previews out of the tab order", () => {
    expect(renderCard(true, htmlDetail)).toContain('tabindex="-1"');
  });

  it("toggles the card when the formatted preview surface is clicked", () => {
    const container = document.createElement("div");
    const onToggle = vi.fn();
    const root = createRoot(container);

    flushSync(() => {
      root.render(
        <EventCard
          copied={false}
          detail={htmlDetail}
          detailFailed={false}
          detailLoading={false}
          expanded
          language="en"
          messages={getMessages("en")}
          onDelete={vi.fn()}
          onRestore={vi.fn()}
          onRetryDetail={vi.fn()}
          onToggle={onToggle}
          restoring={false}
          summary={textSummary}
        />
      );
    });

    container
      .querySelector<HTMLElement>(".event-html-preview-shell")
      ?.dispatchEvent(new window.MouseEvent("click", { bubbles: true }));

    expect(onToggle).toHaveBeenCalledOnce();
    flushSync(() => root.unmount());
  });
});
