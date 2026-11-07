<!-- The app: a header, the map (reloaded on every rebuild in the desktop app) and, in the desktop app, the chat. -->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import type { View } from "../map/types";
  import type { IgApi } from "../api";
  import { ChatSession } from "./chat/session.svelte";
  import Chat from "./chat/Chat.svelte";
  import Map from "../map/Map.svelte";

  let { api }: { api: IgApi } = $props();

  let hint = $state("");
  const msg = (e: unknown) => String((e as Error)?.message ?? e);

  // The map: loaded once, then again on every rebuild. A newer load wins; a failed reload keeps the last map.
  let view = $state.raw<View | null>(null);
  let loadError = $state("");
  let latest = 0;
  async function load() {
    const n = ++latest;
    try {
      const v = await host.view!();
      if (n === latest) { view = v; if (hint === lastErr) hint = ""; lastErr = ""; }
    } catch (e) {
      if (n !== latest) return;
      if (view) hint = lastErr = msg(e); else loadError = msg(e);
    }
  }
  let lastErr = "";
  // an edit's view wins over any older load in flight
  function onview(v: View) { latest++; view = v; }
  // a refused edit (dropAll may have dropped some first): read the map again, and let the caller show why
  const guard = (p: Promise<View>) => p.catch((e) => { void load(); throw e; });
  const edit = untrack(() => {
    const e = api.edit;
    return e && {
      apply: (x: Parameters<typeof e.apply>[0]) => guard(e.apply(x)), keep: (n?: number) => guard(e.keep(n)),
      drop: (n: number) => guard(e.drop(n)), dropAll: () => guard(e.dropAll()),
      undo: () => guard(e.undo()), redo: () => guard(e.redo()),
    };
  });
  const host = untrack(() => api);
  let show = $state.raw(host.shown?.() ?? null);
  const diff = host.diff?.() ?? null;
  onDestroy(host.onShow?.((s) => (show = s)) ?? (() => {}));
  if (host.view) {
    void load();
    const off = host.onRebuild?.(() => void load());
    onDestroy(() => { off?.(); latest = Infinity; });
  }

  /** The chat (desktop app only), docked on the right; "Chat" in the header hides it. */
  let chatOpen = $state(true);
  const chatApi = untrack(() => (api.agent && api.trustInventory && api.trustProject ? api : null));
  const chat = chatApi ? new ChatSession(chatApi.agent!, chatApi.trustInventory!.bind(chatApi), chatApi.trustProject!.bind(chatApi)) : null;
  // a held prompt (behind the trust card or setup) opens a collapsed chat
  $effect(() => {
    if (chat?.held) chatOpen = true;
  });
</script>

<header>
  <h1><svg class="mark" viewBox="0 0 120 120" width="22" height="22" fill="none" stroke="currentColor" stroke-width="5" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true"><path d="M30 42 Q30 22 60 22 Q90 22 90 42 V86 Q90 104 60 104 Q30 104 30 86 Z"/><path d="M35 28 L30 13 L46 23"/><path d="M85 28 L90 13 L74 23"/><path d="M56 62 L60 68 L64 62"/><circle cx="47" cy="48" r="11"/><circle cx="73" cy="48" r="11"/><circle cx="47" cy="48" r="4.5" fill="#E8A317" stroke="none"/><circle cx="73" cy="48" r="4.5" fill="#E8A317" stroke="none"/></svg>ig</h1>
  <span class="spacer" title={hint}>{hint}</span>
  {#if chatApi}<button id="chat-toggle" class="toggle" aria-pressed={chatOpen ? "true" : "false"} onclick={() => (chatOpen = !chatOpen)}>Chat</button>{/if}
</header>
<main>
  <div id="map">
    {#if view}<Map {view} edit={view.landscape || diff ? undefined : edit} {onview} {show} {diff} />{:else if loadError}<p class="map-error">Could not read the map: {loadError}</p>{/if}
  </div>
  {#if chatApi}
    <aside id="chat" hidden={!chatOpen}>
      <Chat chat={chat!} root="" onshow={(s) => (show = { ...s })}
        openUrl={chatApi.openUrl ? (u) => chatApi.openUrl!(u).catch((e: unknown) => (hint = msg(e))) : undefined} />
    </aside>
  {/if}
</main>
