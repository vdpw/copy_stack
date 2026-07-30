// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import {
  buildHtmlPreview,
  buildHtmlPreviewDocument,
  sanitizeHtmlPreview,
} from "./htmlPreview";

describe("sanitizeHtmlPreview", () => {
  it("removes active content, navigation, remote resources, and CSS URLs", () => {
    const sanitized = sanitizeHtmlPreview(`
      <style>@import "https://example.invalid/a.css";</style>
      <script>window.top.location = "https://example.invalid"</script>
      <svg><script>alert(1)</script></svg>
      <a href="https://example.invalid" onclick="alert(1)">safe text</a>
      <p style="color: red; background-image: url(https://example.invalid/x)">
        formatted text
      </p>
      <img src="https://example.invalid/pixel.png" onerror="alert(1)">
    `);

    expect(sanitized).not.toMatch(
      /script|svg|onclick|onerror|example\.invalid|background-image/i
    );
    expect(sanitized).toContain("safe text");
    expect(sanitized).toContain("formatted text");
    expect(sanitized).toMatch(/class="preview-tone-[0-6]"/);
    expect(sanitized).not.toContain("style=");
  });

  it("strips embedded image data and maps safe formatting to classes", () => {
    const sanitized = sanitizeHtmlPreview(
      '<p style="font-weight: 700; position: fixed">Hello</p>' +
        '<img src="data:image/png;base64,iVBORw0KGgo=">'
    );

    expect(sanitized).toContain('class="preview-weight-strong"');
    expect(sanitized).not.toContain("position");
    expect(sanitized).not.toContain("data:image");
    expect(sanitized).not.toContain("src=");
    expect(sanitized).not.toContain("style=");
  });

  it("renders formatted HTML above the former 64 KiB limit", () => {
    const source = "x".repeat(128 * 1024);
    const sanitized = sanitizeHtmlPreview(
      `<div style="background-color: #1e1e1e; white-space: pre">${source}</div>`
    );

    expect(sanitized).toContain('class="preview-code-surface');
    expect(sanitized).toContain(source);
  });

  it("caps output nodes and nesting depth", () => {
    const manyNodes = Array.from(
      { length: 3_000 },
      (_, index) => `<span>${index}</span>`
    ).join("");
    const sanitizedNodes = sanitizeHtmlPreview(manyNodes);
    const parsedNodes = new window.DOMParser().parseFromString(
      sanitizedNodes,
      "text/html"
    );
    expect(parsedNodes.body.querySelectorAll("*").length).toBeLessThanOrEqual(
      2_048
    );

    const deeplyNested = `${"<div>".repeat(80)}safe${"</div>".repeat(80)}`;
    const sanitizedDepth = sanitizeHtmlPreview(deeplyNested);
    let depth = 0;
    let node = new window.DOMParser().parseFromString(
      sanitizedDepth,
      "text/html"
    ).body.firstElementChild;
    while (node) {
      depth += 1;
      node = node.firstElementChild;
    }
    expect(depth).toBeLessThanOrEqual(25);

    const customNesting = `${"<custom-element>".repeat(
      200
    )}hidden${"</custom-element>".repeat(200)}`;
    expect(sanitizeHtmlPreview(customNesting)).not.toContain("hidden");
  });

  it("marks colored preformatted blocks as isolated scroll surfaces", () => {
    const sanitized = sanitizeHtmlPreview(
      '<div style="background-color: #1e1e1e; color: #61afef; white-space: pre">long code</div>'
    );

    expect(sanitized).toContain('class="preview-code-surface');
    expect(sanitized).toContain("preview-whitespace-pre");
    expect(sanitized).toMatch(/preview-tone-[0-6]/);
    expect(sanitized).not.toContain("style=");
  });
});

describe("buildHtmlPreviewDocument", () => {
  it("uses a deny-by-default inner CSP and sandbox-compatible document", () => {
    const document = buildHtmlPreviewDocument("<strong>Hello</strong>");

    expect(document).toContain("default-src 'none'");
    expect(document).toContain("base-uri 'none'");
    expect(document).toContain("form-action 'none'");
    expect(document).toContain("img-src 'none'");
    expect(document).not.toContain("'unsafe-inline'");
    expect(document).toContain("-webkit-user-select: none");
    expect(document).toContain("user-select: none");
    expect(document).toContain("-webkit-user-drag: none");
    expect(document).toContain("overflow: auto");
    expect(document).toContain(".preview-code-surface");
    expect(document).toContain("min-height: calc(100vh - 32px)");
    expect(document).toContain("linear-gradient(145deg, #1f2329");
    expect(document).toContain("<strong>Hello</strong>");
  });

  it("authorizes the preview stylesheet with a CSP hash", () => {
    const document = buildHtmlPreviewDocument("<strong>Hello</strong>");
    const parsed = new window.DOMParser().parseFromString(
      document,
      "text/html"
    );
    const innerPolicy =
      parsed
        .querySelector('meta[http-equiv="Content-Security-Policy"]')
        ?.getAttribute("content") ?? "";

    expect(innerPolicy).toMatch(/style-src 'sha256-[A-Za-z0-9+/]+={0,2}'/);
    expect(innerPolicy).not.toContain("'unsafe-inline'");
  });
});

describe("buildHtmlPreview", () => {
  it("uses a compact viewport for simple single-line formatted content", () => {
    const preview = buildHtmlPreview(
      '<div style="background-color: #1e1e1e; white-space: pre">生效的优惠券</div>'
    );

    expect(preview.compact).toBe(true);
    expect(preview.srcDoc).toContain("生效的优惠券");
  });

  it("keeps the full viewport for multiline or structured content", () => {
    expect(
      buildHtmlPreview('<div style="white-space: pre">first\nsecond</div>')
        .compact
    ).toBe(false);
    expect(
      buildHtmlPreview("<ul><li>first</li><li>second</li></ul>").compact
    ).toBe(false);
  });
});
