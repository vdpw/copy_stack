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

interface NodeBudget {
  remaining: number;
}

export interface HtmlPreviewPresentation {
  compact: boolean;
  srcDoc: string;
}

type DomNode = Parameters<HTMLElement["appendChild"]>[0];
type DomDocument = typeof window.document;

function sanitizeInlineStyle(source: HTMLElement, target: HTMLElement) {
  const style = source.getAttribute("style");
  if (!style) {
    return;
  }

  const parser = window.document.createElement("span");
  parser.setAttribute("style", style);
  const declarations: string[] = [];
  let hasBackgroundColor = false;
  let preservesWhitespace = false;

  for (const property of safeStyleProperties) {
    const value = parser.style.getPropertyValue(property).trim();
    if (value && safeCssValue.test(value)) {
      declarations.push(`${property}: ${value}`);
      hasBackgroundColor ||= property === "background-color";
      preservesWhitespace ||= property === "white-space" && value === "pre";
    }
  }

  if (declarations.length > 0) {
    target.setAttribute("style", declarations.join("; "));
  }
  if (hasBackgroundColor && preservesWhitespace) {
    target.classList.add("preview-code-surface");
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
  sanitizeInlineStyle(sourceElement, target);
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

function previewDocument(sanitizedHtml: string): string {
  const previewStyles = `
    :root {
      color-scheme: light;
      background: #fff;
      overflow: auto;
    }
    * {
      box-sizing: border-box;
      max-width: 100%;
      -webkit-user-select: none;
      user-select: none;
    }
    body {
      margin: 0;
      padding: 16px;
      overflow-wrap: anywhere;
      color: #14213d;
      background: #fff;
      cursor: default;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      line-height: 1.55;
    }
    .preview-code-surface {
      max-width: 100%;
      min-height: calc(100vh - 32px);
      padding-bottom: 14px;
      overflow-x: auto;
      overflow-y: hidden;
    }
    img { height: auto; }
    table { border-collapse: collapse; }
  `;

  return `<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'none'; style-src 'unsafe-inline'; font-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'">
    <style>${previewStyles}</style>
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
