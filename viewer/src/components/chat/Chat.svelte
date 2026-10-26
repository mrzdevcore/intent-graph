<!-- The chat panel (desktop app only): a session with the project's agent. Before any session,
     the trust card (ig-trust); then the setup (no node or adapter), login (ig-login) and stopped
     screens when they apply. The session (state, gate, turns) is shared with the board:
     session.svelte.ts. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { editCounts, groupTools } from "../../model/chat";
  import { contextBlock } from "../../board/ready";
  import Trust from "../Trust.svelte";
  import AgentControls from "./AgentControls.svelte";
  import Composer, { type Selection } from "./Composer.svelte";
  import Markdown from "./Markdown.svelte";
  import PermissionCard from "./PermissionCard.svelte";
  import QuestionCard from "./QuestionCard.svelte";
  import ToolCard from "./ToolCard.svelte";
  import type { ChatSession } from "./session.svelte";
  import type { Show } from "../../model/show";

  let { chat: c, openUrl, root, selection, onopendiff, match, onnode, onshow }: {
    /** The session, shared with the board (session.svelte.ts). */
    chat: ChatSession;
    openUrl?: (url: string) => void;
    root: string;
    selection: Selection | null;
    onopendiff: (path: string, before: string, after: string) => void;
    /** Graph refs in replies: text → node id; `onnode` shows it in the graph. */
    match?: (text: string, code?: boolean) => string | null;
    onnode?: (id: string) => void;
    /** An `ig_show` card's "Show in graph". */
    onshow?: (s: Show) => void;
  } = $props();

  const s = $derived(c.s);
  let attached = $state<Selection>({ ids: [], files: [] });
  let transcript: HTMLElement;

  function load(id: string) {
    if (id === "new") {
      const f = window.prompt("New session. Feature (optional, e.g. login-flow):", "");
      if (f !== null) c.start(f.trim() || undefined);
    } else if (id && id !== s.sessionId) {
      c.load(id);
    }
  }

  /** The text, the attached graph nodes, and the board's selected notes framed as data. */
  async function send(text: string) {
    const refs = attached.ids.map((id) => "node `" + id + "`");
    const body = refs.length ? text + "\n\nAttached: " + refs.join(", ") : text;
    const block = contextBlock(c.chips);
    const files = attached.files;
    attached = { ids: [], files: [] };
    c.chips = [];
    if (!(await c.send(block ? block + "\n\n" + body : body, files))) c.draft ||= text;
  }

  function plan() {
    if (!selection) return;
    const what = selection.ids.length ? selection.ids.join(", ") + " (" + selection.files.join(", ") + ")" : selection.files.join(", ");
    send("/ig:plan " + what);
  }

  onMount(() => {
    const off = c.listen();
    if (!s.sessionId) c.start();
    return off;
  });

  // New items scroll into view while the reader is at the bottom.
  $effect(() => {
    void s.items;
    if (transcript && transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 80)
      queueMicrotask(() => (transcript.scrollTop = transcript.scrollHeight));
  });

  const featureOf = (id: string) => Object.keys(c.features).find((f) => c.features[f] === id);
</script>

<section id="ig-chat" aria-label="Agent chat">
  <!-- Read out, never selected or copied with the messages. -->
  <div class="sr-only" aria-live="polite">{s.live}</div>
  <header class="chat-head">
    <strong class="title">{s.title ?? "Agent"}</strong>
    <select aria-label="Session" value={s.sessionId ?? ""} disabled={c.busy || !!c.trust} onchange={(e) => load(e.currentTarget.value)}>
      {#if !s.sessionId}<option value="">No session</option>{/if}
      {#each c.sessions as x}
        <option value={x.sessionId}>{featureOf(x.sessionId) ? featureOf(x.sessionId) + " · " : ""}{x.title ?? x.sessionId.slice(0, 8)}</option>
      {/each}
      {#if s.sessionId && !c.sessions.some((x) => x.sessionId === s.sessionId)}<option value={s.sessionId}>This session</option>{/if}
      <option value="new">New session…</option>
    </select>
  </header>

  {#if c.trust}
    {#if c.declined}
      <div class="screen"><p>The agent stays off until you trust this project.</p><button onclick={() => (c.declined = false)}>Review again</button></div>
    {:else}
      {#if c.held}<p class="held">After you trust: <code>{c.held.text}</code></p>{/if}
      <Trust state={c.trust} ontrust={(h) => c.onTrust(h)} oncancel={() => c.decline()} />
    {/if}
  {:else if s.setup}
    <div class="screen" id="ig-setup">
      <h2>Set up the agent</h2>
      <pre>{s.setup.instructions}</pre>
      <div class="row">
        {#if s.setup.installable}<button class="primary" disabled={c.busy} onclick={() => c.install()}>{c.installing ? "Installing…" : "Install"}</button>{/if}
        <button disabled={c.busy} onclick={() => c.retry()}>Retry</button>
      </div>
    </div>
  {:else if s.login}
    <div class="screen" id="ig-login">
      <h2>Log in to Claude Code</h2>
      <p>Run <code>claude</code> and <code>/login</code> in a terminal, then retry.</p>
      <button class="primary" disabled={c.busy} onclick={c.restart}>Retry</button>
    </div>
  {/if}

  <div class="transcript" bind:this={transcript}>
    {#if !c.trust && !s.setup && !s.login && !s.items.length}
      <p class="muted empty">{c.busy ? "Starting the agent…" : "Ask the agent about this project, or select a node and press Plan this."}</p>
    {/if}
    {#each groupTools(s.items) as item}
      {#if item.kind === "group"}
        <details class="card tool group">
          <summary><span class="ttitle">{item.label}</span><span class="chip {item.status}">{item.status.replace("_", " ")}</span></summary>
          {#each item.calls as call (call.toolCallId)}<ToolCard {call} {root} {onopendiff} {onshow} />{/each}
        </details>
      {:else if item.kind === "text"}
        <div class="msg {item.role}">
          {#if item.role === "user"}<div class="plain">{item.text}</div>
          {:else}<Markdown text={item.text} {openUrl} {match} {onnode} />{/if}
        </div>
      {:else if item.kind === "tool"}
        <ToolCard call={item.call} {root} {onopendiff} {onshow} />
      {:else if item.kind === "permission"}
        <PermissionCard toolCall={item.toolCall} options={item.options} answered={item.answered} onanswer={(o) => c.answer(item.requestId, o)} />
      {:else if item.kind === "question"}
        <QuestionCard message={item.message} questions={item.questions} answered={item.answered} onreply={(v, sum) => c.reply(item.requestId, v, sum)} />
      {:else if item.kind === "board-edit"}
        {@const here = c.board?.feature === item.feature}
        <div class="card board-edit">
          <span>Edited the board · {editCounts(item, true)}</span>
          <div class="row">
            <button disabled={!here} title={here ? "Select and zoom to them" : `Open ${item.feature}'s board to show them`}
              onclick={() => c.board?.show(item.ids)}>Show</button>
            <button disabled={!here || item.undone} title="Undo Claude's edit, keeping your later ones"
              onclick={() => c.board?.undo(item.step)}>{item.undone ? "Undone" : "Undo"}</button>
          </div>
        </div>
      {:else if item.kind === "error"}
        <div class="msg error" role="alert">{item.text}</div>
      {/if}
    {/each}
    {#if s.stopped}
      <div class="screen stopped" role="alert">
        <p><strong>Agent stopped.</strong> {s.stopped.reason}</p>
        {#if s.stopped.stderr}<pre>{s.stopped.stderr}</pre>{/if}
        <button class="primary" disabled={c.busy} onclick={c.restart}>Restart</button>
      </div>
    {/if}
  </div>
  {#if c.error}<div class="msg error" role="alert">{c.error}</div>{/if}

  {#if !c.trust}
    <Composer commands={s.commands} running={s.running} {selection} bind:attached bind:text={c.draft} chips={c.chips} onsend={send}
      oncancel={c.cancel} onplan={plan} onclearchips={() => (c.chips = [])}>
      {#snippet controls()}<AgentControls config={s.config} usage={s.usage} onset={c.setConfig} />{/snippet}
    </Composer>
  {/if}
</section>
