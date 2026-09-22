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
  is_pinned: false,
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
  summary: HistorySummary = textSummary,
  searchQuery = ""
): string {
  return renderToStaticMarkup(
    <EventCard
      pinning={false}
      onPin={vi.fn()}
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
      searchQuery={searchQuery}
      summary={summary}
    />
  );
}

describe("EventCard", () => {
  it("exposes pin state and keeps pin click and keyboard input separate from expansion", () => {
    const container = document.createElement("div");
    const root = createRoot(container);
    const onPin = vi.fn();
    const onToggle = vi.fn();
    const render = (isPinned: boolean, pinning: boolean) =>
      flushSync(() => {
        root.render(
          <EventCard
            summary={{ ...textSummary, is_pinned: isPinned }}
            detail={undefined}
            detailLoading={false}
            detailFailed={false}
            expanded={false}
            copied={false}
            restoring={false}
            pinning={pinning}
            language="en"
            messages={getMessages("en")}
            onPin={onPin}
            onToggle={onToggle}
            onRetryDetail={vi.fn()}
            onRestore={vi.fn()}
            onDelete={vi.fn()}
          />
        );
      });
    render(false, false);
    const pin = container.querySelector<HTMLElement>(
      'button[aria-label="Pin item"]'
    )!;
    expect(pin.getAttribute("aria-pressed")).toBe("false");
    pin.dispatchEvent(
      new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true })
    );
    pin.click();
    expect(onPin).toHaveBeenCalledOnce();
    expect(onToggle).not.toHaveBeenCalled();
    expect(pin.closest('[role="button"]')).toBeNull();
    expect(container.querySelector("article")?.getAttribute("role")).toBeNull();
    const content = container.querySelector<HTMLElement>(".event-content")!;
    expect(content.getAttribute("aria-expanded")).toBe("false");
    content.dispatchEvent(
      new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true })
    );
    expect(onToggle).toHaveBeenCalledOnce();
    render(true, true);
    const unpin = container.querySelector<HTMLElement>(
      'button[aria-label="Unpin item"]'
    )!;
    expect(unpin.getAttribute("aria-pressed")).toBe("true");
    expect(unpin.hasAttribute("disabled")).toBe(true);
    expect(container.querySelector(".event-pinned-badge")?.textContent).toBe(
      "Pinned"
    );
    unpin.click();
    expect(onPin).toHaveBeenCalledOnce();
    flushSync(() => root.unmount());
  });

  it("shows the event type label when collapsed or expanded", () => {
    expect(renderCard(false)).toContain(">文字</span>");
    expect(renderCard(true)).toContain(">文字</span>");
  });

  it("shows full file and folder paths only in expanded details", () => {
    const paths = [
      "/Users/demo/Documents/项目资料/very-long-directory-name/report <final>.pdf",
      "/Users/demo/Documents/项目资料/Archives",
    ];
    const summary: HistorySummary = {
      ...textSummary,
      data_type: "files and folders",
      has_detail: true,
      display: Array.from(
        new globalThis.TextEncoder().encode(
          JSON.stringify({
            format: "copy_stack.file-items.v1",
            items: [{ type: "file", name: "report <final>.pdf" }],
          })
        )
      ),
      display_truncated: true,
    };
    const detail: HistoryDetail = {
      content_hash: summary.content_hash,
      html_preview: null,
      text_preview: null,
      rich_preview: [],
      file_items: [
        { type: "file", name: "report <final>.pdf", path: paths[0] },
        { type: "folder", name: "Archives", path: paths[1] },
      ],
    };
    const collapsed = new window.DOMParser().parseFromString(
      renderCard(false, detail, vi.fn(), summary),
      "text/html"
    );
    expect(collapsed.querySelector(".event-file-path")).toBeNull();
    expect(collapsed.body.textContent).not.toContain(paths[0]);
    expect(collapsed.body.textContent).not.toContain("Archives");

    const expanded = new window.DOMParser().parseFromString(
      renderCard(true, detail, vi.fn(), summary),
      "text/html"
    );
    expect(
      Array.from(
        expanded.querySelectorAll(".event-file-path"),
        item => item.textContent
      )
    ).toEqual(paths);
    expect(expanded.querySelectorAll(".event-file-item")).toHaveLength(2);
    expect(expanded.querySelector("final")).toBeNull();
  });

  it.each(["file", "folder"])(
    "keeps an unresolved %s visible without fabricating its path",
    type => {
      const document = new window.DOMParser().parseFromString(
        renderCard(
          true,
          {
            content_hash: textSummary.content_hash,
            html_preview: null,
            text_preview: null,
            rich_preview: [],
            file_items: [{ type, name: "Archive", path: null }],
          },
          vi.fn(),
          { ...textSummary, data_type: type, has_detail: true }
        ),
        "text/html"
      );
      expect(document.querySelector(".event-file-item")?.textContent).toBe(
        "Archive"
      );
      expect(document.querySelector(".event-file-path")).toBeNull();
    }
  );

  it("does not render stored source provenance", () => {
    const markup = renderCard(false, undefined, vi.fn(), {
      ...textSummary,
      is_remote_clipboard: true,
      source_bundle_id: "com.apple.Safari",
    });

    expect(markup).not.toContain("com.apple.Safari");
    expect(markup).toContain("来自其他设备");
  });

  it("shows a bounded search excerpt when the collapsed summary hides the match", () => {
    const markup = renderCard(
      false,
      undefined,
      vi.fn(),
      {
        ...textSummary,
        search_preview: "…context around deep-target…",
      },
      "deep-target"
    );

    expect(markup).toContain("event-search-match");
    expect(markup).toContain("匹配内容");
    expect(markup).toContain("deep-target");
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
          pinning={false}
          onPin={vi.fn()}
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
          pinning={false}
          onPin={vi.fn()}
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
