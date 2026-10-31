// The agent's markdown as safe HTML: markdown-it with raw HTML and images off, then DOMPurify
// with web links only. Clicks on links go to the system browser (Chat), never the webview.

import DOMPurify from "dompurify";
import MarkdownIt from "markdown-it";

const md = new MarkdownIt({ html: false, linkify: false }).disable("image");

export function renderMarkdown(text: string): string {
  return DOMPurify.sanitize(md.render(text), {
    FORBID_TAGS: ["img", "style", "form", "input", "button", "iframe"],
    ALLOWED_URI_REGEXP: /^https?:\/\//i,
  });
}
