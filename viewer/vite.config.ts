import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { defineConfig, type Plugin } from "vitest/config";
import { viteSingleFile } from "vite-plugin-singlefile";

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

// One self-contained offline page: all JS and CSS inlined, no chunks. The Rust side fills the
// /*IG_DATA*/ and /*IG_LIVE*/ markers in index.html.
export default defineConfig({
  plugins: [viteSingleFile(), inlineSafe()],
  build: { target: "es2022", chunkSizeWarningLimit: 8000, reportCompressedSize: false },
  test: { include: ["test/**/*.test.ts"] },
});
