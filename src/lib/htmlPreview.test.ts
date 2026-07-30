// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { buildHtmlPreviewDocument, sanitizeHtmlPreview } from "./htmlPreview";

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
    expect(sanitized).toContain('style="color: red"');
  });

  it("strips embedded image data while keeping safe inline formatting", () => {
    const sanitized = sanitizeHtmlPreview(
      '<p style="font-weight: 700; position: fixed">Hello</p>' +
        '<img src="data:image/png;base64,iVBORw0KGgo=">'
    );

    expect(sanitized).toContain("font-weight: 700");
    expect(sanitized).not.toContain("position");
    expect(sanitized).not.toContain("data:image");
    expect(sanitized).not.toContain("src=");
  });

  it("rejects oversized input before parsing", () => {
    expect(sanitizeHtmlPreview(`<p>${"x".repeat(70_000)}</p>`)).toBe("");
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
});

describe("buildHtmlPreviewDocument", () => {
  it("uses a deny-by-default inner CSP and sandbox-compatible document", () => {
    const document = buildHtmlPreviewDocument("<strong>Hello</strong>");

    expect(document).toContain("default-src 'none'");
    expect(document).toContain("base-uri 'none'");
    expect(document).toContain("form-action 'none'");
    expect(document).toContain("img-src 'none'");
    expect(document).toContain("-webkit-user-select: none");
    expect(document).toContain("user-select: none");
    expect(document).toContain("<strong>Hello</strong>");
  });
});
