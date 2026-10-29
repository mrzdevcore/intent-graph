// Code colouring for the map: Shiki (VS Code's grammars) with its JavaScript regex engine, so the
// page stays one offline file. Tokens, never HTML: the map draws them as text. Their colours are
// theme tokens (--code-token-*, in styles/themes/), so every theme colours code its own way.
import { createCssVariablesTheme, createHighlighterCoreSync, type HighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import bash from "@shikijs/langs/shellscript";
import csharp from "@shikijs/langs/csharp";
import css from "@shikijs/langs/css";
import go from "@shikijs/langs/go";
import html from "@shikijs/langs/html";
import java from "@shikijs/langs/java";
import javascript from "@shikijs/langs/javascript";
import json from "@shikijs/langs/json";
import kotlin from "@shikijs/langs/kotlin";
import python from "@shikijs/langs/python";
import rust from "@shikijs/langs/rust";
import scss from "@shikijs/langs/scss";
import sql from "@shikijs/langs/sql";
import toml from "@shikijs/langs/toml";
import tsx from "@shikijs/langs/tsx";
import typescript from "@shikijs/langs/typescript";
import vue from "@shikijs/langs/vue";
import yaml from "@shikijs/langs/yaml";

export type Token = { text: string; color?: string; italic: boolean };

// a language more: its import above, in `langs` below, and its file extensions here
const BY_EXT: Record<string, string> = {
  ts: "typescript", mts: "typescript", cts: "typescript", tsx: "tsx", jsx: "tsx",
  js: "javascript", mjs: "javascript", cjs: "javascript", vue: "vue", go: "go", rs: "rust",
  yml: "yaml", yaml: "yaml", kt: "kotlin", kts: "kotlin", cs: "csharp", java: "java", py: "python",
  json: "json", toml: "toml", html: "html", htm: "html", css: "css", scss: "scss", sql: "sql",
  sh: "shellscript", bash: "shellscript",
};

const theme = createCssVariablesTheme({ name: "ig", variablePrefix: "--code-", fontStyle: true });
let shiki: HighlighterCore | null = null;
// built on first use: compiling the grammars costs a little, and most views never show code
const highlighter = () =>
  (shiki ??= createHighlighterCoreSync({
    themes: [theme],
    langs: [bash, csharp, css, go, html, java, javascript, json, kotlin, python, rust, scss, sql, toml, tsx, typescript, vue, yaml],
    engine: createJavaScriptRegexEngine(),
  }));

/** The language of `text` from `file`: a .vue file's template is Vue, its script TypeScript. */
export function language(file: string, text: string): string | null {
  const ext = file.slice(file.lastIndexOf(".") + 1).toLowerCase();
  const lang = BY_EXT[ext] ?? null;
  return lang === "vue" && !text.trimStart().startsWith("<") ? "typescript" : lang;
}

// ponytail: one entry per snippet shown, never evicted; a map shows a few hundred at most
const cache = new Map<string, Token[][]>();

/** `text`'s lines as coloured tokens; plain lines for a language it does not know. */
export function tokens(file: string, text: string): Token[][] {
  const lang = language(file, text);
  const key = `${lang}\n${text}`;
  let out = cache.get(key);
  if (!out) {
    out = lang
      ? highlighter().codeToTokensBase(text, { lang, theme: "ig" }).map((line) =>
          line.map((t) => ({ text: t.content, color: t.color, italic: !!t.fontStyle && (t.fontStyle & 1) === 1 })))
      : text.split("\n").map((l) => [{ text: l, italic: false }]);
    cache.set(key, out);
  }
  return out;
}
