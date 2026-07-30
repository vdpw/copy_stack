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
  text_preview: null,
  rich_preview: [],
};

const textPreviewDetail: HistoryDetail = {
  content_hash: textSummary.content_hash,
  html_preview: null,
  text_preview: 'package main\n\nfunc main() {\n\tprintln("ready")\n}',
  rich_preview: [],
};

function renderCard(
  expanded: boolean,
  detail: HistoryDetail | undefined = undefined,
  onToggle = vi.fn(),
  summary: HistorySummary = textSummary
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
      summary={summary}
    />
  );
}

describe("EventCard", () => {
  it("shows the event type label when collapsed or expanded", () => {
    expect(renderCard(false)).toContain(">文字</span>");
    expect(renderCard(true)).toContain(">文字</span>");
  });

  it("does not render stored source provenance", () => {
    const markup = renderCard(false, undefined, vi.fn(), {
      ...textSummary,
      is_remote_clipboard: true,
      source_bundle_id: "com.apple.Safari",
    });

    expect(markup).not.toContain("com.apple.Safari");
    expect(markup).toContain("来自其他设备");
  });

  it("keeps formatted previews out of the tab order", () => {
    const markup = renderCard(true, htmlDetail);

    expect(markup).toContain('tabindex="-1"');
    expect(markup).toContain("event-html-preview-compact");
  });

  it("renders a scrollable plain-text fallback for oversized formatted code", () => {
    const document = new window.DOMParser().parseFromString(
      renderCard(true, textPreviewDetail),
      "text/html"
    );
    const preview = document.querySelector(".event-text-preview");

    expect(preview?.textContent).toBe(textPreviewDetail.text_preview);
    expect(
      document.querySelector(".event-text-preview-shell")?.getAttribute("role")
    ).toBe("region");
    expect(document.querySelector("iframe")).toBeNull();
  });

  it("names the formatted preview without a hover tooltip", () => {
    const document = new window.DOMParser().parseFromString(
      renderCard(true, htmlDetail),
      "text/html"
    );
    const frame = document.querySelector("iframe");

    expect(frame?.getAttribute("aria-label")).toBe("格式化剪贴板内容预览");
    expect(frame?.hasAttribute("title")).toBe(false);
  });

  it("keeps formatted preview interaction from collapsing the card", () => {
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

    expect(onToggle).not.toHaveBeenCalled();
    flushSync(() => root.unmount());
  });

  it("keeps text preview scrolling from collapsing the card", () => {
    const container = document.createElement("div");
    const onToggle = vi.fn();
    const root = createRoot(container);

    flushSync(() => {
      root.render(
        <EventCard
          copied={false}
          detail={textPreviewDetail}
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
      .querySelector<HTMLElement>(".event-text-preview-shell")
      ?.dispatchEvent(new window.MouseEvent("click", { bubbles: true }));

    expect(onToggle).not.toHaveBeenCalled();
    flushSync(() => root.unmount());
  });
});
