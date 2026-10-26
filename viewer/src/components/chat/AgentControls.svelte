<!-- The agent's settings in the composer footer: Mode and Model as chip selects, Fast mode as a
     toggle chip, and Effort, Agent (any other option) and the usage in a "⋯" menu. A refused change
     (a bypass mode not confirmed in the native dialog) puts the select back. -->
<script lang="ts">
  import type { ConfigOption } from "../../api";
  import { controls, fastNext, fastOn, type Usage } from "../../model/chat";

  let { config, usage, onset }: {
    config: ConfigOption[];
    usage: Usage | null;
    onset: (id: string, value: string) => Promise<void>;
  } = $props();

  const c = $derived(controls(config));
  let open = $state(false);
  let menu = $state<HTMLElement>();

  function set(o: ConfigOption, el: HTMLSelectElement) {
    onset(o.id, el.value).catch(() => (el.value = o.currentValue));
  }

  const money = (x: { amount: number; currency: string }) => {
    try {
      return new Intl.NumberFormat(undefined, { style: "currency", currency: x.currency }).format(x.amount);
    } catch {
      return x.amount.toFixed(2) + " " + x.currency;
    }
  };

  function away(e: Event) {
    if (open && menu && !menu.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={away} />

{#snippet chip(o: ConfigOption)}
  <select class="chip-select" aria-label={o.name} title={o.name} value={o.currentValue} onchange={(e) => set(o, e.currentTarget)}>
    {#each o.options as x}<option value={x.value} title={x.description}>{x.name}</option>{/each}
  </select>
{/snippet}

<div class="controls">
  {#if c.mode}{@render chip(c.mode)}{/if}
  {#if c.model}{@render chip(c.model)}{/if}
  {#if c.fast}
    <button class="chip-toggle" aria-pressed={fastOn(c.fast) ? "true" : "false"} title={c.fast.name}
      onclick={() => onset(c.fast!.id, fastNext(c.fast!)).catch(() => {})}>Fast</button>
  {/if}
  {#if c.more.length || usage}
    <div class="more" bind:this={menu}>
      <button class="chip-toggle" aria-label="More agent settings" aria-haspopup="true" aria-expanded={open ? "true" : "false"}
        onclick={() => (open = !open)}>⋯</button>
      {#if open}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div class="menu" role="group" aria-label="Agent settings" onkeydown={(e) => e.key === "Escape" && (open = false)}>
          {#each c.more as o}<label>{o.name} {@render chip(o)}</label>{/each}
          {#if usage}
            <div class="usage" aria-label="Usage">
              {#if usage.cost}<span title="Cost of this session">{money(usage.cost)}</span>{/if}
              {#if usage.used !== null && usage.size}
                <meter min="0" max={usage.size} value={usage.used} title="Context: {usage.used} of {usage.size} tokens"></meter>
              {/if}
              {#if usage.rateLimit}
                <span title="Rate limit">limit {usage.rateLimit.status ?? ""}{typeof usage.rateLimit.utilization === "number" ? " " + Math.round(usage.rateLimit.utilization * 100) + "%" : ""}</span>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>
