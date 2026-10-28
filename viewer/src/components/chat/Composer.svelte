<!-- The message box: Enter sends, Shift+Enter is a newline, Esc closes the completions, else stops a
     running turn; `/` completes the session's commands (ig's first; a full name sends as typed). -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { completions } from "../../model/chat";

  let { commands, running, text = $bindable(""), onsend, oncancel, controls }: {
    commands: { name: string; description: string }[];
    running: boolean;
    /** The draft: the chat keeps it while the composer is hidden (the trust card). */
    text?: string;
    onsend: (text: string) => void;
    oncancel: () => void;
    /** The agent's settings, in the footer row next to Send. */
    controls?: Snippet;
  } = $props();

  let active = $state(0);
  /** Esc closed the list: it stays closed until the text changes. */
  let closedAt = $state<string | null>(null);
  const list = $derived(closedAt === text ? [] : completions(commands, text).slice(0, 8));

  function complete(name: string) {
    text = "/" + name + " ";
    active = 0;
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape" && list.length) {
      e.preventDefault();
      closedAt = text;
      return;
    }
    if (e.key === "Escape" && running) {
      e.preventDefault();
      return oncancel();
    }
    // A full command name sends as typed.
    const exact = e.key === "Enter" && list.some((c) => "/" + c.name === text);
    if (list.length && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      active = (active + (e.key === "ArrowDown" ? 1 : list.length - 1)) % list.length;
      return;
    }
    if (list.length && !exact && (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey))) {
      e.preventDefault();
      return complete(list[Math.min(active, list.length - 1)].name);
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (!text.trim() || running) return;
      onsend(text.trim());
      text = "";
    }
  }

</script>

<div class="composer">
  {#if list.length}
    <ul class="complete" role="listbox" aria-label="Commands">
      {#each list as c, i}
        <li role="option" aria-selected={i === active}>
          <button tabindex="-1" onclick={() => complete(c.name)}><strong>/{c.name}</strong> <span class="muted">{c.description}</span></button>
        </li>
      {/each}
    </ul>
  {/if}
  <textarea rows="3" placeholder="Message the agent; / for commands" aria-label="Message" bind:value={text} onkeydown={key}></textarea>
  <div class="row footer">
    {@render controls?.()}
    {#if running}
      <button class="primary" onclick={oncancel}>Stop</button>
    {:else}
      <button class="primary" disabled={!text.trim()} onclick={() => { if (text.trim()) { onsend(text.trim()); text = ""; } }}>Send</button>
    {/if}
  </div>
</div>
