<!-- The trust screen (ig-trust): what a Claude Code session in this project will run, in plain words,
     with Trust and Cancel. Shown before an agent starts, unless the project is already trusted. -->
<script lang="ts">
  import type { TrustState } from "../api";

  let { state, ontrust, oncancel }: {
    state: TrustState;
    ontrust: (hash: string) => void;
    oncancel: () => void;
  } = $props();
</script>

<div id="ig-trust" role="dialog" aria-label="Trust this project" class="trust">
  <h2>Trust this project?</h2>
  <p>Claude Code will run this project's own setup as soon as a session starts.</p>
  {#if state.hooks.length}
    <p>These hooks will run:</p>
    <ul>{#each state.hooks as h}<li>{h.when}: <code>{h.command}</code></li>{/each}</ul>
  {/if}
  {#if state.instructions.length}
    <p>It will follow the instructions in: {#each state.instructions as f, i}{i ? ", " : ""}<code>{f}</code>{/each}</p>
  {/if}
  {#if state.mcp.length}
    <p>These MCP servers may start:</p>
    <ul>{#each state.mcp as m}<li><code>{m}</code></li>{/each}</ul>
  {/if}
  {#each state.notes as n}<p>{n}</p>{/each}
  {#each state.unreadable as f}<p>Could not read <code>{f}</code>.</p>{/each}
  {#if state.ig_plugin}<p>The ig plugin in <code>.claude/skills/ig</code> will be loaded.</p>{/if}
  {#if !state.any_files}
    <p>Nothing in the project runs on its own.</p>
  {/if}
  {#if state.incomplete}<p role="alert"><strong>{state.incomplete}</strong></p>{/if}
  <p>
    {#if !state.incomplete}<button onclick={() => ontrust(state.hash)}>Trust</button>{/if}
    <button onclick={oncancel}>Cancel</button>
  </p>
</div>

<style>
  .trust { padding: 16px; background: var(--panel); color: var(--fg); }
  code { font-family: ui-monospace, Menlo, monospace; }
</style>
