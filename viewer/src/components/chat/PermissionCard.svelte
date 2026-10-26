<!-- The agent asks before a tool runs: the adapter's own options as buttons. Takes the focus. -->
<script lang="ts">
  import { onMount } from "svelte";
  import type { PermissionOption, ToolCallInfo } from "../../api";

  let { toolCall, options, answered, onanswer }: {
    toolCall: ToolCallInfo;
    options: PermissionOption[];
    answered: string | null;
    onanswer: (optionId: string) => void;
  } = $props();

  let card: HTMLElement;
  onMount(() => {
    if (answered === null) card.focus();
  });

  const label = (id: string) =>
    options.find((o) => o.optionId === id)?.name ?? (id === "stopped" ? "Agent stopped" : id === "cancelled" ? "Cancelled" : id);
</script>

<div class="card perm" class:open={answered === null} role="group" aria-label="Permission request" tabindex="-1" bind:this={card}>
  <div class="ask">Allow <strong>{toolCall.title ?? toolCall.kind ?? "this tool"}</strong>?</div>
  {#if answered === null}
    <div class="row">
      {#each options as o}<button class:reject={o.kind.startsWith("reject")} onclick={() => onanswer(o.optionId)}>{o.name}</button>{/each}
    </div>
  {:else}
    <div class="muted">{label(answered)}</div>
  {/if}
</div>
