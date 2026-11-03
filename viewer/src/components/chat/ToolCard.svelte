<!-- A tool call, collapsed: title, a status chip and the location; open it for its output and
     diffs. -->
<script lang="ts">
  import type { ToolCallInfo } from "../../api";
  import { shortCommand } from "../../model/chat";
  import { showFromTool, type Show } from "../../model/show";

  let { call, root, onshow }: { call: ToolCallInfo; root: string; onshow?: (s: Show) => void } = $props();
  const show = $derived(showFromTool(call));
  const count = $derived(show ? show.steps.length || show.nodes.length : 0);

  type Diff = { type: "diff"; path: string; oldText?: string | null; newText: string };
  type Text = { type: "content"; content: { type: string; text?: string } };

  const rel = (p: string) => (root && p.startsWith(root.replace(/\/$/, "") + "/") ? p.slice(root.replace(/\/$/, "").length + 1) : p);
  const content = $derived((call.content ?? []) as (Diff | Text)[]);
  const loc = $derived((call.locations ?? [])[0] as { path: string; line?: number } | undefined);
  const status = $derived(call.status ?? "pending");
</script>

<details class="card tool">
  <summary>
    {#if show}
      <span class="ttitle" title={show.title}>{show.title}</span>
      {#if status === "failed"}<span class="chip failed">refused</span>
      {:else if status === "completed"}
        <span class="chip completed">{count} {count === 1 ? "step" : "steps"}</span>
        {#if onshow}<button type="button" onclick={(e) => { e.preventDefault(); onshow(show); }}>Show again</button>{/if}
      {:else}<span class="chip {status}">drawing</span>{/if}
    {:else}
      <span class="ttitle" title={call.title}>{call.title ? shortCommand(call.title, root) : (call.kind ?? "Tool")}</span>
      <span class="chip {status}">{status.replace("_", " ")}</span>
    {/if}
    {#if loc}<span class="loc">{rel(loc.path)}{loc.line ? ":" + loc.line : ""}</span>{/if}
  </summary>
  {#each content as c}
    {#if c.type === "diff"}
      <div class="diff">
        <div class="row"><code>{rel(c.path)}</code></div>
        <pre>{#each (c.oldText ?? "").split("\n").filter(() => c.oldText) as l}<span class="removed">- {l}{"\n"}</span>{/each}{#each c.newText.split("\n") as l}<span class="added">+ {l}{"\n"}</span>{/each}</pre>
      </div>
    {:else if c.type === "content" && c.content?.text}
      <pre>{c.content.text}</pre>
    {/if}
  {/each}
</details>
