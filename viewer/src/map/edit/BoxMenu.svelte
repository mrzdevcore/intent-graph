<!-- The box menu (right-click, or the bar's ⋯). Arrow keys move, a hint key picks its item, Esc closes (handled here: the map ignores keys while focus is in a field). -->
<script lang="ts">
  import { flushSync, onMount } from "svelte";
  import type { KindView } from "../types";
  import { iconMask } from "./add";
  import { menuKeys, type Act } from "./menu";

  let { x, y, title, group = false, code = false, kinds, current, onpick, onkind, onclose }: {
    x: number; y: number; title: string; group?: boolean; code?: boolean; kinds: KindView[]; current: string;
    onpick: (a: Act) => void; onkind: (name: string) => void; onclose: () => void;
  } = $props();
  const items: [Act, string, string][] = $derived([
    ["rename", "Rename", "F2"], ["kinds", "Kind", "▸"], ["details", "Edit details", "E"], ["arrow", "Arrow from here", "A"], ["add", "Add a box inside", "B"],
    ["move", "Move to a group…", "M"], ...(group ? [["arrange", "Arrange inside", ""] as [Act, string, string]] : []),
    ...(code ? [["code", "Open the code", "O"] as [Act, string, string]] : []), ["delete", "Delete", "⌫"],
  ]);
  const KEYS: Record<string, Act> = { f2: "rename", e: "details", a: "arrow", b: "add", m: "move", o: "code", delete: "delete", backspace: "delete" };
  let menu: HTMLDivElement;
  let sub = $state(false);
  const top = () => [...menu.querySelectorAll<HTMLElement>("[data-item]")];
  const subItems = () => [...menu.querySelectorAll<HTMLElement>("[data-sub] button")];
  onMount(() => top()[0]?.focus());
  function openSub() {
    flushSync(() => (sub = true));
    subItems()[0]?.focus();
  }
  function closeSub() {
    sub = false;
    menu.querySelector<HTMLElement>('[data-item="kinds"]')?.focus();
  }
  function keydown(e: KeyboardEvent) {
    const inSub = !!(e.target as Element).closest?.("[data-sub]"), k = e.key.toLowerCase();
    if (inSub) {
      if (e.key === "ArrowLeft" || e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        closeSub();
      } else if (k === "k") onpick("kinds");
      else menuKeys(e, subItems(), closeSub);
      return;
    }
    if (e.key === "ArrowRight" && (e.target as HTMLElement).dataset.item === "kinds") {
      e.preventDefault();
      openSub();
    } else if (!e.metaKey && !e.ctrlKey && !e.altKey && KEYS[k] && items.some(([a]) => a === KEYS[k])) {
      e.preventDefault();
      onpick(KEYS[k]);
    } else menuKeys(e, top(), onclose);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div role="menu" data-popover aria-label="{title} menu" tabindex="-1" bind:this={menu} style:left="{x}px" style:top="{y}px" onkeydown={keydown}>
  {#each items as [a, label, hint] (a)}
    {#if a === "kinds"}
      <div class="sub-row">
        <button type="button" role="menuitem" data-item={a} aria-haspopup="menu" aria-expanded={sub} onclick={() => (sub ? closeSub() : openSub())}><span>{label}</span><kbd>{hint}</kbd></button>
        {#if sub}
          <div role="menu" aria-label="Kinds" data-sub>
            {#each kinds as k (k.name)}
              <button type="button" role="menuitemradio" aria-checked={k.name === current} data-kind={k.name} onclick={() => onkind(k.name)}>
                <i style:mask-image={iconMask(k.icon)} aria-hidden="true"></i><span>{k.title}</span>
              </button>
            {/each}
            <button type="button" role="menuitem" onclick={() => onpick("kinds")}><span>All kinds…</span><kbd>K</kbd></button>
          </div>
        {/if}
      </div>
    {:else}
      <button type="button" role="menuitem" data-item={a} class:bad={a === "delete"} onclick={() => onpick(a)}><span>{label}</span>{#if hint}<kbd>{hint}</kbd>{/if}</button>
    {/if}
  {/each}
</div>

<style>
  [role="menu"] { position: absolute; z-index: 6; width: 13.75rem; box-sizing: border-box; padding: var(--space-1); background: var(--map-surface); border: 1px solid var(--map-line);
    border-radius: var(--radius-xl); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); font-size: var(--text-md); color: var(--map-fg); }
  .sub-row { position: relative; }
  [data-sub] { left: calc(100% + var(--space-1-5)); top: calc(-1 * var(--space-1)); width: 12rem; }
  button { display: flex; align-items: center; gap: var(--space-2); width: 100%; font: inherit; color: inherit; background: none; border: 0; border-radius: var(--radius-md); padding: var(--space-1-5) var(--space-2-5); cursor: pointer; text-align: left; }
  button > span { flex: 1; }
  button:hover, button:focus-visible { background: var(--map-sunken); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: -2px; }
  [aria-checked="true"] { background: var(--map-tint-gold); }
  i { flex: none; width: 0.875rem; height: 0.875rem; background: currentColor; mask-repeat: no-repeat; mask-position: center; mask-size: contain; color: var(--map-fg-2); }
  .bad { color: var(--map-bad-text); }
  kbd { font: inherit; font-size: var(--text-sm); color: var(--map-faint); }
</style>
