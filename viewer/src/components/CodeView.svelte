<!-- Read-only CodeMirror: one file (line numbers, folding, `line` scrolled into view, a click on a word
     reports it) or, with `merge`, a node's before and after side by side. -->
<script lang="ts">
  import { foldGutter, syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
  import { MergeView } from "@codemirror/merge";
  import { EditorState, type Extension } from "@codemirror/state";
  import { EditorView, lineNumbers } from "@codemirror/view";
  import { languageFor, wordAt } from "../model/code";

  let { path, text = "", merge, line, onword, onclose }: {
    /** Decides the language; for a merge, the node's file. */
    path: string;
    text?: string;
    merge?: { before: string; after: string };
    /** 1-based line to show. */
    line?: number;
    /** A click on a word, with its 1-based line. */
    onword?: (word: string, line: number) => void;
    onclose: () => void;
  } = $props();

  let host: HTMLElement;

  const theme = EditorView.theme({
    "&": { height: "100%", backgroundColor: "var(--bg)", color: "var(--fg)" },
    ".cm-scroller": { fontFamily: "ui-monospace, Menlo, monospace", fontSize: "12.5px" },
    ".cm-gutters": { backgroundColor: "var(--panel)", color: "var(--muted)", border: "0" },
  });

  const base = (): Extension[] => [
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    lineNumbers(),
    foldGutter(),
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
    languageFor(path),
    theme,
  ];

  $effect(() => {
    host.textContent = "";
    if (merge) {
      const m = new MergeView({
        a: { doc: merge.before, extensions: base() },
        b: { doc: merge.after, extensions: base() },
        parent: host,
        collapseUnchanged: { margin: 3, minSize: 6 },
      });
      return () => m.destroy();
    }
    const v = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: text,
        extensions: [
          ...base(),
          EditorView.domEventHandlers({
            click: (e, view) => {
              const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
              if (pos === null || !onword) return;
              const w = wordAt(view.state.doc.toString(), pos);
              if (w) onword(w, view.state.doc.lineAt(pos).number);
            },
          }),
        ],
      }),
    });
    if (line) {
      const at = v.state.doc.line(Math.min(Math.max(line, 1), v.state.doc.lines)).from;
      v.dispatch({ selection: { anchor: at }, effects: EditorView.scrollIntoView(at, { y: "start" }) });
    }
    return () => v.destroy();
  });
</script>

<section id="ig-code" aria-label={merge ? "Before and after" : "Source: " + path}>
  <header><strong>{path}</strong>{merge ? " · before → after" : ""}<button onclick={onclose}>Close</button></header>
  <div class="host" bind:this={host}></div>
</section>
