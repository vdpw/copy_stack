import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { getMessages } from "../../i18n";
import type { HistorySummary } from "../../types";
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

function renderCard(expanded: boolean): string {
  return renderToStaticMarkup(
    <EventCard
      copied={false}
      detail={undefined}
      detailFailed={false}
      detailLoading={false}
      expanded={expanded}
      language="zh-CN"
      messages={getMessages("zh-CN")}
      onDelete={vi.fn()}
      onRestore={vi.fn()}
      onRetryDetail={vi.fn()}
      onToggle={vi.fn()}
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
});
