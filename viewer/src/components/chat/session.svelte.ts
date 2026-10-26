// The chat's session, shared by the chat panel and the board (one session per feature): the
// trust gate, the session start, load and restart, and the turns. The trust card, the setup
// screen and a running turn hold a prompt back; it goes once afterwards, or shows as not sent.

import type { AgentApi, TrustState } from "../../api";
import { chatReducer, initialChat, type ChatAction } from "../../model/chat";

const msg = (e: unknown) => String((e as Error)?.message ?? e);

interface Waiting { feature: string; text: string; onsent?: () => void }

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

  private waiting: Waiting | null = null;
  private idle: (() => void)[] = [];

  constructor(private agent: AgentApi, private trustInventory: () => Promise<TrustState>,
    private trustProject: (hash: string) => Promise<void>) {}

  /** The feature of the current session, if it has one. */
  get feature(): string | undefined {
    const id = this.s.sessionId;
    return id ? Object.keys(this.features).find((f) => this.features[f] === id) : undefined;
  }

  dispatch = (a: ChatAction) => {
    this.s = chatReducer(this.s, a);
    if (!this.s.running) this.idle.splice(0).forEach((f) => f());
  };

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
  async session(run: () => Promise<string | void>, fresh = true) {
    if (this.busy) return;
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
      else if (!this.s.setup) this.error = m;
    } finally {
      this.busy = false;
    }
    // the held prompt: sent now if this is its feature's session, else that session comes first
    const w = this.waiting;
    if (w && this.s.sessionId) {
      this.waiting = null;
      void this.ask(w.feature, w.text, w.onsent);
    }
  }

  /** A new session; for the held prompt's feature when one waits. */
  start = (feature?: string) => {
    const f = feature ?? this.waiting?.feature;
    return this.session(async () => {
      const id = await this.agent.start(f);
      if (f) this.features = { ...this.features, [f]: id };
      return id;
    });
  };

  restart = () => this.session(() => this.agent.restart(), false);

  load(id: string) {
    return this.session(async () => {
      await this.agent.load(id);
      return id;
    });
  }

  refreshList() {
    this.agent.list().then((l) => {
      this.sessions = l.sessions ?? [];
      this.features = { ...this.features, ...(l.features ?? {}) };
    }, () => {});
  }

  async onTrust(hash: string) {
    try {
      await this.trustProject(hash); // the native confirm is Rust's
      this.declined = false;
      await this.start();
    } catch (e) {
      this.error = msg(e);
      await this.trusted();
    }
  }

  /** The trust card's Cancel: a held prompt is shown as not sent. */
  decline() {
    this.declined = true;
    const w = this.waiting;
    this.waiting = null;
    if (w) this.dispatch({ type: "Notice", text: "Not sent: " + w.text });
  }

  async install() {
    this.busy = this.installing = true;
    this.error = "";
    try {
      await this.agent.install();
      this.busy = this.installing = false;
      await this.start();
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
   *  turn. Behind the trust card or the setup screen it waits and goes once afterwards.
   *  `onsent`: just before it goes. */
  async ask(feature: string, text: string, onsent?: () => void): Promise<void> {
    const w = { feature, text, onsent };
    if (this.feature !== feature) {
      const known = this.features[feature];
      this.waiting = w;
      // `session` sends it once the session is on (after this start, or the one in flight)
      await (known ? this.load(known) : this.start(feature));
      return;
    }
    await this.run(w);
  }

  private async run(w: Waiting) {
    while (this.s.running) await new Promise<void>((r) => this.idle.push(r));
    w.onsent?.();
    await this.send(w.text);
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
