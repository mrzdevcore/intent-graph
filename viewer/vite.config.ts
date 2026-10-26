import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { defineConfig, type Plugin } from "vitest/config";
import { viteSingleFile } from "vite-plugin-singlefile";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The page is inlined into one <script>: a "</script" or "<!--" inside the bundle would end or
// derail it, so the build fails on either.
function inlineSafe(): Plugin {
  let page = "";
  return {
    name: "ig-inline-safe",
    apply: "build",
    configResolved(c) {
      page = resolve(c.root, c.build.outDir, "index.html");
    },
    closeBundle() {
      const html = readFileSync(page, "utf8");
      if (html.includes("<!--") || html.split("<script").length !== html.split("</script").length)
        throw new Error(page + ": the inlined bundle contains <!-- or </script");
    },
  };
}

// `ig view` has no boards: the built page must not carry the engine or its glue.
function noBoards(): Plugin {
  let page = "";
  return {
    name: "ig-no-boards",
    apply: "build",
    configResolved(c) {
      page = resolve(c.root, c.build.outDir, "index.html");
    },
    closeBundle() {
      const html = readFileSync(page, "utf8");
      if (html.includes("__wbindgen") || html.includes("ig-board") || html.includes("ig_board"))
        throw new Error(page + ": board code leaked into the ig view page");
    },
  };
}

// `bun run build`: one self-contained offline page (`ig view`), all JS and CSS inlined, no
// chunks and no boards (`__BOARDS__` false, so they are dead code). The Rust side fills the
// /*IG_DATA*/ and /*IG_LIVE*/ markers in index.html.
// `bun run build:app` (`--mode app`): the desktop app's dist-app/, multi-file, with the board in a
// lazy chunk and its .wasm as an asset.
export default defineConfig(({ mode }) => {
  const app = mode === "app";
  return {
    plugins: app ? [svelte()] : [svelte(), viteSingleFile(), inlineSafe(), noBoards()],
    define: { __BOARDS__: JSON.stringify(app) },
    build: { target: "es2022", outDir: app ? "dist-app" : "dist", chunkSizeWarningLimit: 8000, reportCompressedSize: false },
    // Vitest resolves svelte to its browser build, so a component test can `mount`.
    resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
    test: { include: ["test/**/*.test.ts"] },
  };
});
