const maxHtmlPreviewNodes = 2_048;
const maxHtmlPreviewDepth = 24;

const allowedElements = new Set([
  "a",
  "b",
  "blockquote",
  "br",
  "code",
  "del",
  "div",
  "em",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "hr",
  "i",
  "img",
  "ins",
  "li",
  "ol",
  "p",
  "pre",
  "s",
  "span",
  "strike",
  "strong",
  "sub",
  "sup",
  "table",
  "tbody",
  "td",
  "tfoot",
  "th",
  "thead",
  "tr",
  "u",
  "ul",
]);
const blockedElements = new Set([
  "base",
  "button",
  "canvas",
  "embed",
  "form",
  "frame",
  "frameset",
  "iframe",
  "input",
  "link",
  "math",
  "meta",
  "noscript",
  "object",
  "option",
  "script",
  "select",
  "style",
  "svg",
  "textarea",
]);
const safeAttributes = new Set(["alt", "colspan", "dir", "rowspan", "title"]);
const safeStyleProperties = new Set([
  "background-color",
  "color",
  "font-style",
  "font-weight",
  "text-align",
  "text-decoration",
  "white-space",
]);
const safeCssValue =
  /^(?:#[0-9a-f]{3,8}|rgba?\([\d\s.,%]+\)|hsla?\([\d\s.,%a-z-]+\)|[a-z-]+|[1-9]00|normal|bold|bolder|lighter|italic|oblique|left|right|center|justify|underline|line-through|pre|pre-wrap)$/i;
const previewToneCount = 7;

interface NodeBudget {
  remaining: number;
}

export interface HtmlPreviewPresentation {
  compact: boolean;
  srcDoc: string;
}

type DomNode = Parameters<HTMLElement["appendChild"]>[0];
type DomDocument = typeof window.document;

function parseSafeStyleDeclarations(style: string): Map<string, string> {
  const declarations = new Map<string, string>();

  for (const declaration of style.split(";")) {
    const separator = declaration.indexOf(":");
    if (separator <= 0) {
      continue;
    }

    const property = declaration.slice(0, separator).trim().toLowerCase();
    const value = declaration
      .slice(separator + 1)
      .trim()
      .toLowerCase();
    if (safeStyleProperties.has(property) && safeCssValue.test(value)) {
      declarations.set(property, value);
    }
  }

  return declarations;
}

function presentationTone(value: string): number {
  let hash = 2_166_136_261;
  for (const character of value) {
    hash ^= character.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 16_777_619);
  }
  return (hash >>> 0) % previewToneCount;
}

function applySafePresentationClasses(
  source: HTMLElement,
  target: HTMLElement,
  tagName: string
) {
  const style = source.getAttribute("style");
  const declarations = style
    ? parseSafeStyleDeclarations(style)
    : new Map<string, string>();
  const backgroundColor = declarations.get("background-color");
  const color = declarations.get("color");
  const whiteSpace = declarations.get("white-space");
  const isCodeSurface =
    tagName === "pre" ||
    (Boolean(backgroundColor) &&
      (whiteSpace === "pre" || whiteSpace === "pre-wrap"));

  if (isCodeSurface) {
    target.classList.add("preview-code-surface");
  } else if (backgroundColor) {
    target.classList.add("preview-highlight");
  }

  if (color) {
    target.classList.add(`preview-tone-${presentationTone(color)}`);
  }

  const fontWeight = declarations.get("font-weight");
  if (
    fontWeight === "bold" ||
    fontWeight === "bolder" ||
    (Number.parseInt(fontWeight ?? "", 10) || 0) >= 700
  ) {
    target.classList.add("preview-weight-strong");
  } else if ((Number.parseInt(fontWeight ?? "", 10) || 0) >= 500) {
    target.classList.add("preview-weight-medium");
  }

  const fontStyle = declarations.get("font-style");
  if (fontStyle === "italic" || fontStyle === "oblique") {
    target.classList.add("preview-italic");
  }

  const textAlign = declarations.get("text-align");
  if (["center", "right", "justify"].includes(textAlign ?? "")) {
    target.classList.add(`preview-align-${textAlign}`);
  }

  const textDecoration = declarations.get("text-decoration");
  if (textDecoration === "underline") {
    target.classList.add("preview-underline");
  } else if (textDecoration === "line-through") {
    target.classList.add("preview-line-through");
  }

  if (whiteSpace === "pre" || whiteSpace === "pre-wrap") {
    target.classList.add(`preview-whitespace-${whiteSpace}`);
  }
}

function cloneSafeNode(
  source: DomNode,
  targetParent: HTMLElement,
  targetDocument: DomDocument,
  depth: number,
  budget: NodeBudget
): void {
  if (budget.remaining <= 0 || depth > maxHtmlPreviewDepth) {
    return;
  }

  if (source.nodeType === 3) {
    budget.remaining -= 1;
    targetParent.appendChild(
      targetDocument.createTextNode(source.textContent ?? "")
    );
    return;
  }
  if (source.nodeType !== 1) {
    return;
  }

  const sourceElement = source as HTMLElement;
  const tagName = sourceElement.tagName.toLowerCase();
  if (blockedElements.has(tagName)) {
    return;
  }
  if (!allowedElements.has(tagName)) {
    for (const child of Array.from(sourceElement.childNodes)) {
      cloneSafeNode(child, targetParent, targetDocument, depth + 1, budget);
    }
    return;
  }

  budget.remaining -= 1;
  const target = targetDocument.createElement(tagName);
  for (const attribute of Array.from(sourceElement.attributes)) {
    const attributeName = attribute.name.toLowerCase();
    if (safeAttributes.has(attributeName)) {
      target.setAttribute(attributeName, attribute.value.slice(0, 1_024));
    }
  }
  applySafePresentationClasses(sourceElement, target, tagName);
  targetParent.appendChild(target);

  for (const child of Array.from(sourceElement.childNodes)) {
    cloneSafeNode(child, target, targetDocument, depth + 1, budget);
    if (budget.remaining <= 0) {
      break;
    }
  }
}

function sanitizedHtmlPreviewBody(html: string): HTMLElement {
  const parsed = new window.DOMParser().parseFromString(html, "text/html");
  const safeDocument = window.document.implementation.createHTMLDocument("");
  const budget: NodeBudget = { remaining: maxHtmlPreviewNodes };
  for (const child of Array.from(parsed.body.childNodes)) {
    cloneSafeNode(child, safeDocument.body, safeDocument, 0, budget);
    if (budget.remaining <= 0) {
      break;
    }
  }
  return safeDocument.body;
}

function isCompactHtmlPreview(body: HTMLElement): boolean {
  const text = body.textContent?.trim() ?? "";
  if (text.length === 0) {
    return true;
  }
  if (/[\r\n]/.test(text)) {
    return false;
  }
  if (
    body.querySelector(
      "blockquote, br, hr, img, li, ol, table, tbody, td, tfoot, th, thead, tr, ul"
    )
  ) {
    return false;
  }

  const flowElementCount = body.querySelectorAll(
    "div, h1, h2, h3, h4, h5, h6, p, pre"
  ).length;
  if (flowElementCount > 2) {
    return false;
  }

  const isPreformatted = Boolean(
    body.querySelector(".preview-code-surface, pre")
  );
  return isPreformatted || Array.from(text).length <= 80;
}

export function sanitizeHtmlPreview(html: string): string {
  return sanitizedHtmlPreviewBody(html).innerHTML;
}

const htmlPreviewStyles = `
:root {
  color-scheme: light;
  background: #ffffff;
  overflow: auto;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

html {
  min-height: 100%;
  background: #ffffff;
}

*,
*::before,
*::after {
  box-sizing: border-box;
  max-width: 100%;
  -webkit-user-drag: none;
  -webkit-user-select: none !important;
  user-select: none !important;
}

::selection {
  color: inherit;
  background: transparent;
}

body {
  min-height: 100vh;
  margin: 0;
  padding: 16px;
  overflow-wrap: anywhere;
  color: #24324a;
  background: #ffffff;
  cursor: default;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  font-size: 14px;
  line-height: 1.6;
  text-rendering: optimizeLegibility;
}

body > :first-child {
  margin-top: 0;
}

body > :last-child {
  margin-bottom: 0;
}

h1,
h2,
h3,
h4,
h5,
h6 {
  margin: 1.1em 0 0.45em;
  color: #14213d;
  font-weight: 750;
  line-height: 1.25;
  letter-spacing: -0.015em;
}

h1 {
  font-size: 1.55rem;
}

h2 {
  padding-bottom: 0.25em;
  border-bottom: 1px solid #e2e8f0;
  font-size: 1.3rem;
}

h3 {
  font-size: 1.12rem;
}

h4,
h5,
h6 {
  font-size: 1rem;
}

p,
blockquote,
pre,
table,
ul,
ol {
  margin: 0 0 0.9em;
}

ul,
ol {
  padding-left: 1.55em;
}

li + li {
  margin-top: 0.28em;
}

blockquote {
  padding: 0.65em 0.9em;
  border-left: 3px solid #fb7185;
  border-radius: 0 8px 8px 0;
  color: #475569;
  background: #fff7f8;
}

hr {
  height: 1px;
  margin: 1.15em 0;
  border: 0;
  background: #e2e8f0;
}

strong,
b {
  color: #14213d;
  font-weight: 750;
}

em,
i {
  color: #475569;
}

a {
  color: #0e7490;
  font-weight: 650;
  text-decoration: underline;
  text-decoration-color: #67e8f9;
  text-underline-offset: 0.16em;
}

code {
  padding: 0.12em 0.35em;
  border: 1px solid #e2e8f0;
  border-radius: 5px;
  color: #be123c;
  background: #fff1f2;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
  font-size: 0.88em;
}

pre,
.preview-code-surface {
  width: 100%;
  min-height: calc(100vh - 32px);
  margin: 0;
  padding: 16px 18px 20px;
  overflow: auto;
  border: 1px solid #303844;
  border-radius: 10px;
  color: #d9e2f1;
  background: linear-gradient(145deg, #1f2329 0%, #171a20 100%);
  box-shadow:
    inset 0 1px 0 rgb(255 255 255 / 7%),
    0 8px 24px rgb(15 23 42 / 12%);
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
  font-size: 0.82rem;
  line-height: 1.6;
  overflow-wrap: normal;
  tab-size: 2;
  white-space: pre !important;
}

pre code,
.preview-code-surface code {
  padding: 0;
  border: 0;
  color: inherit;
  background: transparent;
  font: inherit;
}

.preview-code-surface * {
  font-family: inherit;
}

table {
  width: 100%;
  overflow: hidden;
  border: 1px solid #dbe3ec;
  border-radius: 8px;
  border-collapse: separate;
  border-spacing: 0;
  font-size: 0.93em;
}

th,
td {
  padding: 0.55em 0.7em;
  border-right: 1px solid #e2e8f0;
  border-bottom: 1px solid #e2e8f0;
  text-align: left;
  vertical-align: top;
}

th {
  color: #14213d;
  background: #f1f5f9;
  font-weight: 700;
}

tr:last-child > td {
  border-bottom: 0;
}

th:last-child,
td:last-child {
  border-right: 0;
}

img {
  display: none;
}

.preview-highlight {
  padding: 0.05em 0.22em;
  border-radius: 4px;
  background: #fef3c7;
  box-decoration-break: clone;
}

.preview-weight-medium {
  font-weight: 600;
}

.preview-weight-strong {
  font-weight: 750;
}

.preview-italic {
  font-style: italic;
}

.preview-align-center {
  text-align: center;
}

.preview-align-right {
  text-align: right;
}

.preview-align-justify {
  text-align: justify;
}

.preview-underline {
  text-decoration: underline;
  text-underline-offset: 0.14em;
}

.preview-line-through {
  text-decoration: line-through;
}

.preview-whitespace-pre {
  overflow-wrap: normal;
  white-space: pre !important;
}

.preview-whitespace-pre-wrap {
  white-space: pre-wrap !important;
}

.preview-tone-0 {
  color: #0369a1;
}

.preview-tone-1 {
  color: #0f766e;
}

.preview-tone-2 {
  color: #3f6212;
}

.preview-tone-3 {
  color: #a16207;
}

.preview-tone-4 {
  color: #c2410c;
}

.preview-tone-5 {
  color: #be123c;
}

.preview-tone-6 {
  color: #7e22ce;
}

.preview-code-surface .preview-tone-0,
.preview-code-surface.preview-tone-0 {
  color: #61afef;
}

.preview-code-surface .preview-tone-1,
.preview-code-surface.preview-tone-1 {
  color: #56b6c2;
}

.preview-code-surface .preview-tone-2,
.preview-code-surface.preview-tone-2 {
  color: #98c379;
}

.preview-code-surface .preview-tone-3,
.preview-code-surface.preview-tone-3 {
  color: #e5c07b;
}

.preview-code-surface .preview-tone-4,
.preview-code-surface.preview-tone-4 {
  color: #d19a66;
}

.preview-code-surface .preview-tone-5,
.preview-code-surface.preview-tone-5 {
  color: #e06c75;
}

.preview-code-surface .preview-tone-6,
.preview-code-surface.preview-tone-6 {
  color: #c678dd;
}

::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  border: 3px solid transparent;
  border-radius: 999px;
  background: #94a3b8;
  background-clip: padding-box;
}
`;
const htmlPreviewStyleHash =
  "sha256-DV6gCx9H/0zifNgu3LZB1nySQPY8jTLVmOECY7WolhM=";

function previewDocument(sanitizedHtml: string): string {
  return `<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'none'; style-src '${htmlPreviewStyleHash}'; font-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'">
    <style>${htmlPreviewStyles}</style>
  </head>
  <body>${sanitizedHtml}</body>
</html>`;
}

export function buildHtmlPreview(html: string): HtmlPreviewPresentation {
  const body = sanitizedHtmlPreviewBody(html);
  return {
    compact: isCompactHtmlPreview(body),
    srcDoc: previewDocument(body.innerHTML),
  };
}

export function buildHtmlPreviewDocument(html: string): string {
  return buildHtmlPreview(html).srcDoc;
}
