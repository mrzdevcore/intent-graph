<!-- The agent's question (AskUserQuestion): its options as buttons, an "Other" box, Skip. One
     single-choice question answers on the click; otherwise pick, then Send. Takes the focus. -->
<script lang="ts">
  import { onMount } from "svelte";
  import type { Question } from "../../model/chat";

  let { message, questions, answered, onreply }: {
    message: string;
    questions: Question[];
    answered: string | null;
    onreply: (content: Record<string, string | string[]> | null, summary: string) => void;
  } = $props();

  let picks = $state<Record<string, string[]>>({});
  let other = $state<Record<string, string>>({});
  let card: HTMLElement;
  onMount(() => {
    if (answered === null) card.focus();
  });

  const quick = $derived(questions.length === 1 && !questions[0].multi);

  function pick(q: Question, value: string) {
    if (quick) return send({ [q.field]: value });
    const cur = picks[q.field] ?? [];
    picks[q.field] = q.multi ? (cur.includes(value) ? cur.filter((v) => v !== value) : [...cur, value]) : [value];
  }

  function send(given?: Record<string, string | string[]>) {
    const c: Record<string, string | string[]> = given ?? {};
    if (!given)
      for (const q of questions) {
        const p = picks[q.field] ?? [];
        if (p.length) c[q.field] = q.multi ? p : p[0];
      }
    for (const q of questions) if (q.custom && other[q.field]?.trim()) c[q.custom] = other[q.field].trim();
    onreply(c, Object.values(c).flat().join(", ") || "no answer");
  }
</script>

<div class="card question" class:open={answered === null} role="group" aria-label="Question from the agent" tabindex="-1" bind:this={card}>
  <div class="ask">{message}</div>
  {#each questions as q}
    {#if q.title || (questions.length > 1 && q.description)}<div class="qtitle">{q.title ?? ""}{q.description && questions.length > 1 ? ": " + q.description : ""}</div>{/if}
    {#if answered === null}
      <div class="row">
        {#each q.options as o}
          <button title={o.description} aria-pressed={quick ? undefined : (picks[q.field] ?? []).includes(o.value)} onclick={() => pick(q, o.value)}>{o.title}</button>
        {/each}
      </div>
      {#if q.custom}<input class="other" placeholder="Other…" aria-label="Other answer" bind:value={other[q.field]} />{/if}
    {/if}
  {/each}
  {#if answered === null}
    <div class="row">
      <button onclick={() => send()}>Send</button>
      <button onclick={() => onreply(null, "skipped")}>Skip</button>
    </div>
  {:else}
    <div class="muted">{answered === "stopped" ? "Agent stopped" : answered}</div>
  {/if}
</div>
