import { expect, test } from "vitest";
import { language, tokens } from "../src/map/highlight";

test("the language comes from the file; a .vue file's script is TypeScript", () => {
  expect(language("src/a.ts", "")).toBe("typescript");
  expect(language("main.go", "")).toBe("go");
  expect(language("deploy/site.YML", "")).toBe("yaml");
  expect(language("App.vue", "<template>\n</template>")).toBe("vue");
  expect(language("App.vue", "function save() {}")).toBe("typescript");
  expect(language("Makefile", "")).toBeNull();
});

test("tokens: theme colours, one list per line, the text kept whole", () => {
  const code = "const x = 1; // one\nfunction f() { return 'z' }";
  const lines = tokens("a.ts", code);
  expect(lines.map((l) => l.map((t) => t.text).join(""))).toEqual(code.split("\n"));
  const kw = lines[0].find((t) => t.text === "const")!;
  expect(kw.color).toMatch(/^var\(--code-token-/);
  expect(lines[0].find((t) => t.text.includes("one"))!.color).toBe("var(--code-token-comment)");
  const vue = tokens("App.vue", '<template>\n  <div :a="b">{{ c }}</div>\n</template>');
  expect(vue).toHaveLength(3);
  expect(vue[1].some((t) => t.color && t.color !== "var(--code-foreground)")).toBe(true);
});

test("a language it does not know: plain lines", () => {
  expect(tokens("Makefile", "all:\n\tgo build")).toEqual([[{ text: "all:", italic: false }], [{ text: "\tgo build", italic: false }]]);
});
