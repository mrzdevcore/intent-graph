// @vitest-environment happy-dom
// Agent text is untrusted: no raw HTML, no images, web links only.
import { expect, test } from "vitest";
import { linkNodes, nodeMatcher, renderMarkdown } from "../src/model/markdown";

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

const nodes = [
  { id: "front/core/router.ts", name: "router.ts" },
  { id: "front/core/router.ts::homeRoute", name: "homeRoute" },
  { id: "micro/a.ts::setup", name: "setup" },
  { id: "micro/b.ts::setup", name: "setup" },
];
const match = nodeMatcher(nodes, "/Users/me/shop-b2b");

test("graph refs: an id, a file (relative, absolute, with a line), a unique name", () => {
  expect(match("front/core/router.ts::homeRoute")).toBe("front/core/router.ts::homeRoute");
  expect(match("front/core/router.ts")).toBe("front/core/router.ts");
  expect(match("/Users/me/shop-b2b/front/core/router.ts:12")).toBe("front/core/router.ts");
  expect(match("homeRoute", true)).toBe("front/core/router.ts::homeRoute");
  expect(match("homeRoute")).toBeNull(); // names only in inline code
  expect(match("setup", true)).toBeNull(); // not unique
  expect(match("router")).toBeNull();
});

test("sanitized markdown gets node buttons, code blocks and links stay as they are", () => {
  const el = document.createElement("div");
  el.innerHTML = renderMarkdown(
    "Routing (`front/core/router.ts` → front/core/router.ts::homeRoute), `homeRoute` and `setup`.\n\n" +
    "```mermaid\ngraph TD; homeRoute-->x\n```\n\n[front/core/router.ts](https://x.test/front/core/router.ts)");
  linkNodes(el, match);
  const buttons = [...el.querySelectorAll("button[data-node]")];
  expect(buttons.map((b) => [b.getAttribute("data-node"), b.textContent])).toEqual([
    ["front/core/router.ts", "front/core/router.ts"],
    ["front/core/router.ts::homeRoute", "front/core/router.ts::homeRoute"],
    ["front/core/router.ts::homeRoute", "homeRoute"],
  ]);
  expect(el.querySelector("pre code")!.textContent).toContain("homeRoute-->x");
  expect(el.querySelector("a")!.textContent).toBe("front/core/router.ts");
  expect(el.textContent).toContain("homeRoute), ");
});
