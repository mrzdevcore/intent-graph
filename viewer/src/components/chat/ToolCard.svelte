<!-- A tool call, collapsed: title, a status chip and the location; open it for its output and
     diffs (CodeMirror merge view, with "Open in code view"). -->
<script lang="ts">
  import type { ToolCallInfo } from "../../api";
  import { shortCommand } from "../../model/chat";
  import { showFromTool, type Show } from "../../model/show";
  import CodeView from "../CodeView.svelte";

  let { call, root, onopendiff, onshow }: {
    call: ToolCallInfo;
    root: string;
    onopendiff: (path: string, before: string, after: string) => void;
    /** "Show in graph" on an `ig_show` card: its input, shown again. */
    onshow?: (s: Show) => void;
  } = $props();

  type Diff = { type: "diff"; path: string; oldText?: string | null; newText: string };
  type Text = { type: "content"; content: { type: string; text?: string } };

  const rel = (p: string) => (root && p.startsWith(root.replace(/\/$/, "") + "/") ? p.slice(root.replace(/\/$/, "").length + 1) : p);
  const content = $derived((call.content ?? []) as (Diff | Text)[]);
  const loc = $derived((call.locations ?? [])[0] as { path: string; line?: number } | undefined);
  const status = $derived(call.status ?? "pending");
  const show = $derived(onshow ? showFromTool(call) : null);
</script>

<details class="card tool">
  <summary>
    <span class="ttitle" title={call.title}>{call.title ? shortCommand(call.title, root) : (call.kind ?? "Tool")}</span>
    <span class="chip {status}">{status.replace("_", " ")}</span>
    {#if show}<button class="chip" title="Show this answer in the graph again" onclick={(e) => { e.preventDefault(); onshow!(show); }}>Show in graph</button>{/if}
    {#if loc}<span class="loc">{rel(loc.path)}{loc.line ? ":" + loc.line : ""}</span>{/if}
  </summary>
  {#each content as c}
    {#if c.type === "diff"}
      <div class="diff">
        <div class="row"><code>{rel(c.path)}</code><button onclick={() => onopendiff(c.path, c.oldText ?? "", c.newText)}>Open in code view</button></div>
        <CodeView inline path={c.path} merge={{ before: c.oldText ?? "", after: c.newText }} />
      </div>
    {:else if c.type === "content" && c.content?.text}
      <pre>{c.content.text}</pre>
    {/if}
  {/each}
</details>
