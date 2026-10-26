<!-- The chat panel (desktop app only): a session with the project's agent. Before any session,
     the trust card (ig-trust); then the setup (no node or adapter), login (ig-login) and stopped
     screens when they apply. The state is `chatReducer` over the agent's events. -->
<script lang="ts">
  import { onMount } from "svelte";
  import type { AgentApi, TrustState } from "../../api";
  import { chatReducer, groupTools, initialChat, type ChatAction } from "../../model/chat";
  import Trust from "../Trust.svelte";
  import AgentControls from "./AgentControls.svelte";
  import Composer, { type Selection } from "./Composer.svelte";
  import Markdown from "./Markdown.svelte";
  import PermissionCard from "./PermissionCard.svelte";
  import QuestionCard from "./QuestionCard.svelte";
  import ToolCard from "./ToolCard.svelte";
  import type { Show } from "../../model/show";

  let { agent, trustInventory, trustProject, openUrl, root, selection, onopendiff, match, onnode, onshow }: {
    agent: AgentApi;
    trustInventory: () => Promise<TrustState>;
    trustProject: (hash: string) => Promise<void>;
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

  let s = $state.raw(initialChat);
  const dispatch = (a: ChatAction) => (s = chatReducer(s, a));
  let trust = $state.raw<TrustState | null>(null);
  let declined = $state(false);
  let error = $state("");
  let busy = $state(false);
  let sessions = $state.raw<{ sessionId: string; title?: string }[]>([]);
  let features = $state.raw<Record<string, string>>({});
  let attached = $state<Selection>({ ids: [], files: [] });
  /** The composer's draft: a send stopped by the trust card puts its text back. */
  let draft = $state("");
  let transcript: HTMLElement;

  const msg = (e: unknown) => String((e as Error)?.message ?? e);

  /** The gate first: an untrusted project shows the trust card and starts nothing. */
  async function trusted(): Promise<boolean> {
    const t = await trustInventory();
    trust = t.trusted ? null : t;
    return t.trusted;
  }

  /** `run` (a start, load or restart) behind the trust gate, one at a time. */
  async function session(run: () => Promise<string | void>, fresh = true) {
    if (busy) return;
    busy = true;
    error = "";
    try {
      if (!(await trusted())) return;
      if (fresh) dispatch({ type: "Reset" });
      const id = await run();
      if (id) dispatch({ type: "Started", sessionId: id });
      refreshList();
    } catch (e) {
      const m = msg(e);
      if (m.startsWith("not trusted")) await trusted();
      else if (!s.setup) error = m;
    } finally {
      busy = false;
    }
  }

  const start = (feature?: string) => session(() => agent.start(feature));

  function refreshList() {
    agent.list().then((l) => {
      sessions = l.sessions ?? [];
      features = l.features ?? {};
    }, () => {});
  }

  function load(id: string) {
    if (id === "new") {
      const f = window.prompt("New session. Feature (optional, e.g. login-flow):", "");
      if (f !== null) start(f.trim() || undefined);
    } else if (id && id !== s.sessionId) {
      session(async () => {
        await agent.load(id);
        return id;
      });
    }
  }

  async function onTrust(hash: string) {
    try {
      await trustProject(hash); // the native confirm is Rust's
      start();
    } catch (e) {
      error = msg(e);
      await trusted();
    }
  }

  async function install() {
    busy = true;
    error = "";
    try {
      await agent.install();
      busy = false;
      start();
    } catch (e) {
      error = msg(e).startsWith("not installed") ? "" : msg(e); // declined in the native dialog
      busy = false;
    }
  }

  async function send(text: string) {
    if (busy) return void (error = "The agent is starting; send again in a moment.");
    if (!s.sessionId) await start();
    if (!s.sessionId) return void (draft ||= text);
    const refs = attached.ids.map((id) => "node `" + id + "`");
    const full = refs.length ? text + "\n\nAttached: " + refs.join(", ") : text;
    const files = attached.files;
    attached = { ids: [], files: [] };
    dispatch({ type: "Sent", text: full });
    try {
      dispatch({ type: "TurnEnded", stopReason: await agent.prompt(full, files) });
    } catch (e) {
      if (msg(e).startsWith("not trusted")) {
        draft ||= text;
        await trusted();
        return;
      }
      if (!s.stopped) dispatch({ type: "Failed", error: msg(e) });
      else dispatch({ type: "TurnEnded", stopReason: "stopped" });
    }
  }

  function plan() {
    if (!selection) return;
    const what = selection.ids.length ? selection.ids.join(", ") + " (" + selection.files.join(", ") + ")" : selection.files.join(", ");
    send("/ig:plan " + what);
  }

  /** Sends an answer; the card closes when it went through, or when the agent no longer waits for it. */
  async function settle(requestId: number, answer: string, send: () => Promise<void>) {
    try {
      await send();
    } catch (e) {
      error = msg(e);
      if (!/already answered or unknown/.test(error)) return; // the card stays open: try again
    }
    dispatch({ type: "Answered", requestId, answer });
  }

  const answer = (requestId: number, optionId: string) =>
    settle(requestId, optionId, () => agent.answer(requestId, optionId));

  const reply = (requestId: number, content: Record<string, string | string[]> | null, summary: string) =>
    settle(requestId, summary, () => agent.reply(requestId, content));

  /** A refused change (a bypass mode not confirmed) rejects: the control goes back. */
  const setConfig = (id: string, value: string) =>
    agent.setConfig(id, value).catch((e) => {
      error = msg(e);
      throw e;
    });

  onMount(() => {
    const off = agent.onEvent(dispatch);
    start();
    return off;
  });

  // New items scroll into view while the reader is at the bottom.
  $effect(() => {
    void s.items;
    if (transcript && transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 80)
      queueMicrotask(() => (transcript.scrollTop = transcript.scrollHeight));
  });

  const featureOf = (id: string) => Object.keys(features).find((f) => features[f] === id);
</script>

<section id="ig-chat" aria-label="Agent chat">
  <!-- Read out, never selected or copied with the messages. -->
  <div class="sr-only" aria-live="polite">{s.live}</div>
  <header class="chat-head">
    <strong class="title">{s.title ?? "Agent"}</strong>
    <select aria-label="Session" value={s.sessionId ?? ""} disabled={busy || !!trust} onchange={(e) => load(e.currentTarget.value)}>
      {#if !s.sessionId}<option value="">No session</option>{/if}
      {#each sessions as x}
        <option value={x.sessionId}>{featureOf(x.sessionId) ? featureOf(x.sessionId) + " · " : ""}{x.title ?? x.sessionId.slice(0, 8)}</option>
      {/each}
      {#if s.sessionId && !sessions.some((x) => x.sessionId === s.sessionId)}<option value={s.sessionId}>This session</option>{/if}
      <option value="new">New session…</option>
    </select>
  </header>

  {#if trust}
    {#if declined}
      <div class="screen"><p>The agent stays off until you trust this project.</p><button onclick={() => (declined = false)}>Review again</button></div>
    {:else}
      <Trust state={trust} ontrust={onTrust} oncancel={() => (declined = true)} />
    {/if}
  {:else if s.setup}
    <div class="screen" id="ig-setup">
      <h2>Set up the agent</h2>
      <pre>{s.setup.instructions}</pre>
      <div class="row">
        {#if s.setup.installable}<button class="primary" disabled={busy} onclick={install}>{busy ? "Installing…" : "Install"}</button>{/if}
        <button disabled={busy} onclick={() => start()}>Retry</button>
      </div>
    </div>
  {:else if s.login}
    <div class="screen" id="ig-login">
      <h2>Log in to Claude Code</h2>
      <p>Run <code>claude</code> and <code>/login</code> in a terminal, then retry.</p>
      <button class="primary" disabled={busy} onclick={() => session(() => agent.restart(), false)}>Retry</button>
    </div>
  {/if}

  <div class="transcript" bind:this={transcript}>
    {#if !trust && !s.setup && !s.login && !s.items.length}
      <p class="muted empty">{busy ? "Starting the agent…" : "Ask the agent about this project, or select a node and press Plan this."}</p>
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
        <PermissionCard toolCall={item.toolCall} options={item.options} answered={item.answered} onanswer={(o) => answer(item.requestId, o)} />
      {:else if item.kind === "question"}
        <QuestionCard message={item.message} questions={item.questions} answered={item.answered} onreply={(c, sum) => reply(item.requestId, c, sum)} />
      {:else}
        <div class="msg error" role="alert">{item.text}</div>
      {/if}
    {/each}
    {#if s.stopped}
      <div class="screen stopped" role="alert">
        <p><strong>Agent stopped.</strong> {s.stopped.reason}</p>
        {#if s.stopped.stderr}<pre>{s.stopped.stderr}</pre>{/if}
        <button class="primary" disabled={busy} onclick={() => session(() => agent.restart(), false)}>Restart</button>
      </div>
    {/if}
  </div>
  {#if error}<div class="msg error" role="alert">{error}</div>{/if}

  {#if !trust}
    <Composer commands={s.commands} running={s.running} {selection} bind:attached bind:text={draft} onsend={send}
      oncancel={() => agent.cancel().catch((e) => (error = msg(e)))} onplan={plan}>
      {#snippet controls()}<AgentControls config={s.config} usage={s.usage} onset={setConfig} />{/snippet}
    </Composer>
  {/if}
</section>
