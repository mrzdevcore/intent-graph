// Boots the viewer: reads the data the Rust side embedded (or, under `bun run dev`, viewer/dev-data.json)
// and mounts <ig-app>. With `ig view --watch`, reloads when the server says the page was rebuilt.

import "./styles/theme.css";
import "./styles/app.css";
import "./components/ig-app";
import type { IgApp } from "./components/ig-app";
import type { ViewData } from "./data";

async function load(): Promise<ViewData> {
  const raw = document.getElementById("ig-data")!.textContent!.trim();
  if (import.meta.env.DEV && raw === "/*IG_DATA*/") return (await fetch("/dev-data.json")).json();
  return JSON.parse(raw);
}

const live = document.getElementById("ig-live")!.textContent!.trim() === "true";
load().then((data) => {
  (document.querySelector("ig-app") as IgApp).data = data;
});

if (live && window.EventSource) {
  new EventSource("/events").onmessage = () => location.reload();
}
