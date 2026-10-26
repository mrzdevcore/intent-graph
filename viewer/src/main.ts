// Boots the viewer: reads the data the Rust side embedded (or, under `bun run dev`, viewer/dev-data.json)
// and mounts <App>. With `ig view --watch`, reloads when the server says the page was rebuilt.

import { mount } from "svelte";
import "./styles/theme.css";
import "./styles/app.css";
import App from "./components/App.svelte";
import type { ViewData } from "./data";
import { client } from "./model/notes";

async function load(): Promise<ViewData> {
  const raw = document.getElementById("ig-data")!.textContent!.trim();
  if (import.meta.env.DEV && raw === "/*IG_DATA*/") return (await fetch("/dev-data.json")).json();
  return JSON.parse(raw);
}

const liveRaw = document.getElementById("ig-live")!.textContent!.trim();
const live: { token: string } | false = liveRaw.startsWith("{") ? JSON.parse(liveRaw) : false;
let app: ReturnType<typeof App> | undefined;
load().then((data) => {
  app = mount(App, { target: document.getElementById("app")!, props: { data, api: live ? client(live.token) : undefined } });
});

if (live && window.EventSource) {
  const es = new EventSource("/api/events");
  es.addEventListener("rebuild", () => location.reload());
  es.addEventListener("note", () => app?.refreshNotes());
  es.addEventListener("focus", (e) => app?.focusAll(JSON.parse((e as MessageEvent).data)));
}
