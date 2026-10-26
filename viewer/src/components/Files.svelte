<!-- Every file of the project (not only graph modules), loaded one directory at a time, with a filter and
     arrow-key navigation. `path` is highlighted (graph → files); a click calls `onselect` (files → graph). -->
<script lang="ts">
  import type { FileEntry } from "../api";

  let { list, path, version, onselect }: {
    list: (dir?: string) => Promise<FileEntry[]>;
    /** The file to highlight: its ancestors open. */
    path: string | null;
    /** Changes when the project's data does: the loaded directories are dropped. */
    version: unknown;
    onselect: (path: string) => void;
  } = $props();

  let filter = $state("");
  let sel = $state<string | null>(null);
  let kids = $state.raw<Record<string, FileEntry[]>>({});
  let open = $state.raw<Record<string, boolean>>({});
  let error = $state("");
  let body: HTMLElement;

  const ROOT = "";
  const name = (p: string) => p.slice(p.lastIndexOf("/") + 1);

  async function load(dir: string) {
    try {
      const es = await list(dir || undefined);
      kids = { ...kids, [dir]: es };
      error = "";
    } catch (e) {
      error = String((e as Error)?.message ?? e);
    }
  }

  // New data: start over from the root, keeping what was open.
  $effect(() => {
    void version;
    kids = {};
    load(ROOT);
    Object.keys(open).filter((d) => open[d]).forEach(load);
  });

  // Graph → files: open the ancestors of `path` and select it.
  $effect(() => {
    if (!path) return;
    const parts = path.split("/").slice(0, -1);
    const dirs = parts.map((_, i) => parts.slice(0, i + 1).join("/"));
    open = { ...open, ...Object.fromEntries(dirs.map((d) => [d, true])) };
    dirs.filter((d) => !kids[d]).forEach(load);
    sel = path;
  });

  type Row = { e: FileEntry; depth: number };
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const f = filter.trim().toLowerCase();
    const walk = (dir: string, depth: number) => {
      for (const e of kids[dir] ?? []) {
        if (e.kind === "dir") {
          if (open[e.path] && !f) {
            out.push({ e, depth });
            walk(e.path, depth + 1);
          } else if (!f || name(e.path).toLowerCase().includes(f)) out.push({ e, depth });
          else if (open[e.path]) walk(e.path, depth + 1); // a filtered match below an open directory
        } else if (!f || name(e.path).toLowerCase().includes(f)) out.push({ e, depth });
      }
    };
    walk(ROOT, 0);
    return out;
  });

  function toggle(d: string, on = !open[d]) {
    open = { ...open, [d]: on };
    if (on && !kids[d]) load(d);
  }

  function pick(r: Row) {
    sel = r.e.path;
    if (r.e.kind === "dir") toggle(r.e.path);
    else onselect(r.e.path);
  }

  function onKey(ev: KeyboardEvent) {
    const i = rows.findIndex((r) => r.e.path === sel);
    const r = rows[i];
    const go = (j: number) => {
      sel = rows[Math.max(0, Math.min(rows.length - 1, j))]?.e.path ?? null;
      ev.preventDefault();
    };
    if (ev.key === "ArrowDown") go(i + 1);
    else if (ev.key === "ArrowUp") go(i < 0 ? 0 : i - 1);
    else if (ev.key === "ArrowRight" && r?.e.kind === "dir") toggle(r.e.path, true);
    else if (ev.key === "ArrowLeft" && r?.e.kind === "dir") toggle(r.e.path, false);
    else if (ev.key === "Enter" && r) pick(r);
    else return;
    ev.preventDefault();
  }

  $effect(() => {
    void [rows, sel];
    body.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
  });
</script>

<section id="ig-files" aria-label="Project files">
  <div class="pane-h">Files</div>
  <input id="files-filter" type="search" placeholder="Filter files in the tree…" aria-label="Filter project files"
    autocomplete="off" bind:value={filter}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div id="files-body" role="tree" tabindex="0" bind:this={body} onkeydown={onKey}>
    {#each rows as r (r.e.path)}
      {@const s = r.e.path === sel}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
      <div class="row{r.e.kind === 'dir' ? ' dir' : ''}{s ? ' sel' : ''}" role="treeitem" aria-selected={s ? "true" : "false"}
          style="padding-left:{8 + r.depth * 12}px" title={r.e.path} onclick={() => pick(r)}>
        <span class="caret">{r.e.kind === "dir" ? (open[r.e.path] ? "▾" : "▸") : ""}</span>
        <span class="name">{name(r.e.path)}</span>
      </div>
    {:else}
      <p class="loc">  {error || "Nothing matches."}</p>
    {/each}
  </div>
</section>
