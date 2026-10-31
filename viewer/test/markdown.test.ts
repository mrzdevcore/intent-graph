// @vitest-environment happy-dom
// Agent text is untrusted: no raw HTML, no images, web links only.
import { expect, test } from "vitest";
import { renderMarkdown } from "../src/model/markdown";

test("markdown renders, raw HTML stays text", () => {
  const h = renderMarkdown("**bold** <img src=x onerror=alert(1)><script>alert(2)</script>");
  expect(h).toContain("<strong>bold</strong>");
  expect(h).not.toMatch(/<img|<script/);
  expect(h).toContain("&lt;img");
});

test("no remote images, no script links", () => {
  const h = renderMarkdown("![logo](https://evil.test/x.png) [a](javascript:alert(1)) [b](https://ok.test/p) [c](file:///etc/passwd)");
  expect(h).not.toContain("<img");
  expect(h).not.toMatch(/href="(javascript|file):/);
  expect(h).toContain('href="https://ok.test/p"');
});
