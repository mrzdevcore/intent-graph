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

// `pnpm run build`: one self-contained offline page (`ig view`), all JS and CSS inlined, no
// chunks. The Rust side fills the /*IG_DATA*/ and /*IG_LIVE*/ markers in index.html.
// `pnpm run build:app` (`--mode app`): the desktop app's dist-app/, multi-file.
export default defineConfig(({ mode }) => {
  const app = mode === "app";
  return {
    plugins: app ? [svelte()] : [svelte(), viteSingleFile(), inlineSafe()],
    build: { target: "es2022", outDir: app ? "dist-app" : "dist", chunkSizeWarningLimit: 8000, reportCompressedSize: false },
    // Vitest resolves svelte to its browser build, so a component test can `mount`.
    resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
    test: { include: ["test/**/*.test.ts"] },
  };
});
