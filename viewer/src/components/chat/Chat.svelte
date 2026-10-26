<!-- The chat panel (desktop app only): a session with the project's agent. Before any session,
     the trust card (ig-trust); then the setup (no node or adapter), login (ig-login) and stopped
     screens when they apply. The state is `chatReducer` over the agent's events. -->
<script lang="ts">
  import { onMount } from "svelte";
  import type { AgentApi, TrustState } from "../../api";
  import { chatReducer, initialChat, type ChatAction } from "../../model/chat";
  import Trust from "../Trust.svelte";
  import Composer, { type Selection } from "./Composer.svelte";
  import Markdown from "./Markdown.svelte";
  import PermissionCard from "./PermissionCard.svelte";
  import QuestionCard from "./QuestionCard.svelte";
  import ToolCard from "./ToolCard.svelte";

  let { agent, trustInventory, trustProject, openUrl, root, selection, onopendiff }: {
    agent: AgentApi;
    trustInventory: () => Promise<TrustState>;
    trustProject: (hash: string) => Promise<void>;
    openUrl?: (url: string) => void;
    root: string;
    selection: Selection | null;
    onopendiff: (path: string, before: string, after: string) => void;
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
    if (!window.confirm("Install the agent adapter into ig's data folder with npm?")) return;
    busy = true;
    error = "";
    try {
      await agent.install();
      busy = false;
      start();
    } catch (e) {
      error = msg(e);
      busy = false;
    }
  }

  async function send(text: string) {
    if (busy) return void (error = "The agent is starting; send again in a moment.");
    if (!s.sessionId) await start();
    if (!s.sessionId) return;
    const refs = attached.ids.map((id) => "node `" + id + "`");
    const full = refs.length ? text + "\n\nAttached: " + refs.join(", ") : text;
    const files = attached.files;
    attached = { ids: [], files: [] };
    dispatch({ type: "Sent", text: full });
    try {
      dispatch({ type: "TurnEnded", stopReason: await agent.prompt(full, files) });
    } catch (e) {
      if (!s.stopped) dispatch({ type: "Failed", error: msg(e) });
      else dispatch({ type: "TurnEnded", stopReason: "stopped" });
    }
  }

  function plan() {
    if (!selection) return;
    const what = selection.ids.length ? selection.ids.join(", ") + " (" + selection.files.join(", ") + ")" : selection.files.join(", ");
    send("/ig:plan " + what);
  }

  async function answer(requestId: number, optionId: string) {
    try {
      await agent.answer(requestId, optionId);
    } catch (e) {
      error = msg(e);
    }
    dispatch({ type: "Answered", requestId, answer: optionId });
  }

  async function reply(requestId: number, content: Record<string, string | string[]> | null, summary: string) {
    try {
      await agent.reply(requestId, content);
    } catch (e) {
      error = msg(e);
    }
    dispatch({ type: "Answered", requestId, answer: summary });
  }

  function setConfig(id: string, value: string) {
    agent.setConfig(id, value).catch((e) => (error = msg(e)));
  }

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

  const money = (c: { amount: number; currency: string }) => {
    try {
      return new Intl.NumberFormat(undefined, { style: "currency", currency: c.currency }).format(c.amount);
    } catch {
      return c.amount.toFixed(2) + " " + c.currency;
    }
  };
  const featureOf = (id: string) => Object.keys(features).find((f) => features[f] === id);
</script>

<section id="ig-chat" aria-label="Agent chat">
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
  {#if s.config.length || s.usage}
    <div class="chat-bar">
      {#each s.config as c}
        <label>{c.name}
          <select value={c.currentValue} onchange={(e) => setConfig(c.id, e.currentTarget.value)}>
            {#each c.options as o}<option value={o.value} title={o.description}>{o.name}</option>{/each}
          </select>
        </label>
      {/each}
      {#if s.usage}
        <span class="usage" aria-label="Usage">
          {#if s.usage.cost}<span title="Cost of this session">{money(s.usage.cost)}</span>{/if}
          {#if s.usage.used !== null && s.usage.size}
            <meter min="0" max={s.usage.size} value={s.usage.used} title="Context: {s.usage.used} of {s.usage.size} tokens"></meter>
          {/if}
          {#if s.usage.rateLimit}
            <span title="Rate limit">limit {s.usage.rateLimit.status ?? ""}{typeof s.usage.rateLimit.utilization === "number" ? " " + Math.round(s.usage.rateLimit.utilization * 100) + "%" : ""}</span>
          {/if}
        </span>
      {/if}
    </div>
  {/if}

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
    {#each s.items as item}
      {#if item.kind === "text"}
        <div class="msg {item.role}">
          {#if item.role === "user"}<div class="plain">{item.text}</div>
          {:else}<Markdown text={item.text} {openUrl} />{/if}
        </div>
      {:else if item.kind === "tool"}
        <ToolCard call={item.call} {root} {onopendiff} />
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
  <div class="sr-only" aria-live="polite">{#each s.live as c}<span>{c}</span>{/each}</div>
  {#if error}<div class="msg error" role="alert">{error}</div>{/if}

  {#if !trust}
    <Composer commands={s.commands} running={s.running} {selection} bind:attached onsend={send}
      oncancel={() => agent.cancel().catch((e) => (error = msg(e)))} onplan={plan} />
  {/if}
</section>
