// What the code view needs besides CodeMirror: the editor mode by extension, a diff's before/after
// text, and the graph node behind a clicked word.

import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import type { Extension } from "@codemirror/state";
import type { DiffLine, ViewNode } from "../data";

const BY_EXT: Record<string, () => Extension> = {
  ts: () => javascript({ typescript: true }),
  mts: () => javascript({ typescript: true }),
  cts: () => javascript({ typescript: true }),
  tsx: () => javascript({ typescript: true, jsx: true }),
  js: () => javascript(),
  mjs: () => javascript(),
  cjs: () => javascript(),
  jsx: () => javascript({ jsx: true }),
  css: () => css(),
  html: () => html(),
  htm: () => html(),
  vue: () => html(), // ponytail: Vue as plain HTML; add @codemirror/lang-vue when needed
  json: () => json(),
};

const ext = (path: string) => path.slice(path.lastIndexOf(".") + 1).toLowerCase();

/** The language mode of `path`, chosen by extension; none for other files (plain text). */
export const languageFor = (path: string): Extension[] => {
  const f = path.includes(".") ? BY_EXT[ext(path)] : undefined;
  return f ? [f()] : [];
};

/** The text before (context and `-` rows) and after (context and `+` rows) of a node's diff. */
export function beforeAfter(diff: DiffLine[]) {
  const side = (keep: string) => diff.filter((d) => d[0] === " " || d[0] === keep).map((d) => d[3]).join("\n");
  return { before: side("-"), after: side("+") };
}

/** The non-module node of `file` that `word` names; of several, the one whose lines contain `line`. */
export function nodeNamed(nodes: ViewNode[], file: string, word: string, line: number): ViewNode | undefined {
  const named = nodes.filter((n) => n.file === file && !n.module && n.name === word);
  return named.find((n) => n.line <= line && line <= n.endLine) ?? named[0];
}
