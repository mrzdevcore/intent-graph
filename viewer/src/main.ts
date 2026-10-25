// Boots the viewer: reads the data the Rust side embedded (or, under `bun run dev`, viewer/dev-data.json)
// and mounts <ig-app>. With `ig view --watch`, reloads when the server says the page was rebuilt.

import "./styles/theme.css";
import "./styles/app.css";
import "./components/ig-app";
import type { IgApp } from "./components/ig-app";
import type { ViewData } from "./data";
import { client } from "./model/notes";

async function load(): Promise<ViewData> {
  const raw = document.getElementById("ig-data")!.textContent!.trim();
  if (import.meta.env.DEV && raw === "/*IG_DATA*/") return (await fetch("/dev-data.json")).json();
  return JSON.parse(raw);
}

const app = document.querySelector("ig-app") as IgApp;
const liveRaw = document.getElementById("ig-live")!.textContent!.trim();
const live: { token: string } | false = liveRaw.startsWith("{") ? JSON.parse(liveRaw) : false;
load().then((data) => {
  app.data = data;
  if (live) app.live(client(live.token));
});

if (live && window.EventSource) {
  const es = new EventSource("/api/events");
  es.addEventListener("rebuild", () => location.reload());
  es.addEventListener("note", () => app.refreshNotes());
  es.addEventListener("focus", (e) => app.focusAll(JSON.parse((e as MessageEvent).data)));
}
