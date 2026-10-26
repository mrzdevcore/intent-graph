// The chat's session, shared by the chat panel and the board (one session per feature): the
// trust gate, the session start, load and restart, and the turns. The board's prompt waits
// behind the trust card or the setup screen (held: shown on the card) and goes once afterwards;
// anything else that starts or loads a session, a second prompt, or an error drops it, visibly
// ("Not sent: …"). A running turn holds every prompt back until it ends.

import type { AgentApi, TrustState } from "../../api";
import type { Outbox } from "../../board/outbox";
import { chatReducer, initialChat, type ChatAction } from "../../model/chat";

const msg = (e: unknown) => String((e as Error)?.message ?? e);

export interface Held { feature: string; text: string }

export class ChatSession {
  s = $state.raw(initialChat);
  /** The trust card's state while the project is not trusted. */
  trust = $state.raw<TrustState | null>(null);
  declined = $state(false);
  error = $state("");
  /** A start, load or restart runs (also an install). */
  busy = $state(false);
  /** Only while `agent.install` runs. */
  installing = $state(false);
  sessions = $state.raw<{ sessionId: string; title?: string }[]>([]);
  /** Feature slug → session id (`.ig/sessions.json`). */
  features = $state.raw<Record<string, string>>({});
  /** The composer's draft: a send stopped by the trust card puts its text back. */
  draft = $state("");
  /** The board's selected sticky notes, sent along with the next message as data. */
  chips = $state.raw<{ id: string; text: string }[]>([]);

  /** The board's prompt waiting for the trust card or the setup screen. */
  held = $state.raw<Held | null>(null);
  /** Board edits per feature: they wait for the feature's session and survive the board closing. */
  outboxes = new Map<string, Outbox>();

  private idle: (() => void)[] = [];
  private starting: Promise<void> | null = null;

  constructor(private agent: AgentApi, private trustInventory: () => Promise<TrustState>,
    private trustProject: (hash: string) => Promise<void>) {}

  /** The feature of the current session, if it has one. */
  get feature(): string | undefined {
    const id = this.s.sessionId;
    return id ? Object.keys(this.features).find((f) => this.features[f] === id) : undefined;
  }

  dispatch = (a: ChatAction) => {
    this.s = chatReducer(this.s, a);
    // a turn's end, a failure, a reset or a stop: the waiting prompts go (or see the session gone)
    if (!this.s.running) this.idle.splice(0).forEach((f) => f());
    this.sync();
  };

  /** Each feature's outbox: sends only in its feature's session, never during a turn. */
  sync() {
    const f = this.feature;
    for (const [slug, o] of this.outboxes) {
      o.setRunning(this.s.running);
      o.setLive(f === slug);
    }
  }

  private setFeatures(f: Record<string, string>) {
    this.features = f;
    this.sync();
  }

  /** The held prompt dropped, visibly. */
  private drop() {
    const w = this.held;
    this.held = null;
    if (w) this.dispatch({ type: "Notice", text: "Not sent: " + w.text });
  }

  /** The agent's events; returns the unsubscribe. */
  listen(): () => void {
    return this.agent.onEvent(this.dispatch);
  }

  /** The gate first: an untrusted project shows the trust card and starts nothing. */
  async trusted(): Promise<boolean> {
    const t = await this.trustInventory();
    this.trust = t.trusted ? null : t;
    return t.trusted;
  }

  /** `run` (a start, load or restart) behind the trust gate, one at a time. A held prompt goes
   *  once the session is on. */
  session(run: () => Promise<string | void>, fresh = true): Promise<void> {
    if (this.busy) return Promise.resolve();
    const p = this.open(run, fresh);
    this.starting = p.finally(() => (this.starting = null));
    return p;
  }

  private async open(run: () => Promise<string | void>, fresh: boolean) {
    this.busy = true;
    this.error = "";
    try {
      if (!(await this.trusted())) return;
      if (fresh) this.dispatch({ type: "Reset" });
      const id = await run();
      if (id) this.dispatch({ type: "Started", sessionId: id });
      this.refreshList();
    } catch (e) {
      const m = msg(e);
      if (m.startsWith("not trusted")) await this.trusted();
      else if (!this.s.setup) {
        this.error = m;
        this.drop(); // the setup screen keeps it (Retry, Install); any other error does not
      }
    } finally {
      this.busy = false;
    }
    // the held prompt: sent now if this is its feature's session, else that session comes first
    const w = this.held;
    if (w && this.s.sessionId) {
      this.held = null;
      void this.ask(w.feature, w.text);
    }
  }

  private begin(f?: string) {
    return this.session(async () => {
      const id = await this.agent.start(f);
      if (f) this.setFeatures({ ...this.features, [f]: id });
      return id;
    });
  }

  /** `run` with the held prompt dropped unless `keep` (said once the new transcript is up). */
  private async instead(keep: boolean, run: () => Promise<void>) {
    const lost = this.held && !keep ? this.held : null;
    if (lost) this.held = null;
    await run();
    if (lost) this.dispatch({ type: "Notice", text: "Not sent: " + lost.text });
  }

  /** A new session (the user's). A held prompt for another feature is dropped. */
  start = (feature?: string) => this.instead(feature === this.held?.feature, () => this.begin(feature));

  /** The setup screen's Retry and Install, and the trust card: the held prompt's feature. */
  retry = () => this.begin(this.held?.feature);

  restart = () => this.instead(false, () => this.session(() => this.agent.restart(), false));

  private loadRun(id: string) {
    return this.session(async () => {
      await this.agent.load(id);
      return id;
    });
  }

  /** Another session (the user's pick). A held prompt for another feature is dropped. */
  load(id: string) {
    return this.instead(!!this.held && this.features[this.held.feature] === id, () => this.loadRun(id));
  }

  refreshList() {
    this.agent.list().then((l) => {
      this.sessions = l.sessions ?? [];
      this.setFeatures({ ...this.features, ...(l.features ?? {}) });
    }, () => {});
  }

  /** The board opened `feature`: its session comes up when the chat has nothing else going
   *  (no session, or a fresh one with no feature and an empty transcript). */
  async adopt(feature: string) {
    await this.starting;
    const idle = !this.s.sessionId || (!this.feature && !this.s.items.length && !this.s.running);
    if (!idle || this.held || this.trust || this.busy) return;
    const l = await this.agent.list().catch(() => null);
    if (l) this.setFeatures({ ...this.features, ...(l.features ?? {}) });
    const id = this.features[feature];
    if (id && id !== this.s.sessionId && !this.busy && !this.held) await this.loadRun(id);
  }

  async onTrust(hash: string) {
    try {
      await this.trustProject(hash); // the native confirm is Rust's
      this.declined = false;
      await this.retry();
    } catch (e) {
      this.error = msg(e);
      await this.trusted();
    }
  }

  /** The trust card's Cancel: a held prompt is shown as not sent. */
  decline() {
    this.declined = true;
    this.drop();
  }

  async install() {
    this.busy = this.installing = true;
    this.error = "";
    try {
      await this.agent.install();
      this.busy = this.installing = false;
      await this.retry();
    } catch (e) {
      this.error = msg(e).startsWith("not installed") ? "" : msg(e); // declined in the native dialog
      this.busy = this.installing = false;
    }
  }

  /** One turn in the current session (started first if none). False when nothing was sent. */
  async send(text: string, files: string[] = []): Promise<boolean> {
    if (this.busy) {
      this.error = "The agent is starting; send again in a moment.";
      return false;
    }
    if (!this.s.sessionId) await this.start();
    if (!this.s.sessionId) return false;
    this.dispatch({ type: "Sent", text });
    try {
      this.dispatch({ type: "TurnEnded", stopReason: await this.agent.prompt(text, files) });
    } catch (e) {
      if (msg(e).startsWith("not trusted")) {
        await this.trusted();
        return false;
      }
      if (!this.s.stopped) this.dispatch({ type: "Failed", error: msg(e) });
      else this.dispatch({ type: "TurnEnded", stopReason: "stopped" });
    }
    return true;
  }

  /** The board: `text` in the feature's session (loaded or started first), after any running
   *  turn. Behind the trust card or the setup screen it is held and goes once afterwards; a
   *  second prompt replaces it. */
  async ask(feature: string, text: string): Promise<void> {
    if (this.held) this.drop();
    if (this.feature !== feature) {
      const known = this.features[feature];
      this.held = { feature, text };
      // `open` sends it once the session is on (after this start, or the one in flight)
      await (known ? this.loadRun(known) : this.begin(feature));
      return;
    }
    await this.post(text);
  }

  /** `text` in the current session once its turn ends; dropped, visibly, when the session
   *  went away (a stop, another session) in the meantime. */
  async post(text: string): Promise<void> {
    const sid = this.s.sessionId;
    while (this.s.running) await new Promise<void>((r) => this.idle.push(r));
    if (!sid || this.s.sessionId !== sid || this.s.stopped) {
      this.dispatch({ type: "Notice", text: "Not sent: " + text });
      return;
    }
    await this.send(text);
  }

  /** Sends an answer; the card closes when it went through, or when the agent no longer waits for it. */
  private async settle(requestId: number, answer: string, send: () => Promise<void>) {
    try {
      await send();
    } catch (e) {
      this.error = msg(e);
      if (!/already answered or unknown/.test(this.error)) return; // the card stays open: try again
    }
    this.dispatch({ type: "Answered", requestId, answer });
  }

  answer = (requestId: number, optionId: string) =>
    this.settle(requestId, optionId, () => this.agent.answer(requestId, optionId));

  reply = (requestId: number, content: Record<string, string | string[]> | null, summary: string) =>
    this.settle(requestId, summary, () => this.agent.reply(requestId, content));

  /** A refused change (a bypass mode not confirmed) rejects: the control goes back. */
  setConfig = (id: string, value: string) =>
    this.agent.setConfig(id, value).catch((e) => {
      this.error = msg(e);
      throw e;
    });

  cancel = () => this.agent.cancel().catch((e) => (this.error = msg(e)));
}
