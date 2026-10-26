// Boots the viewer: picks how to reach the data (inline JSON, `ig serve`, or the desktop app), reads it
// once and mounts <App>, which follows the host's rebuilds from then on.

import { mount } from "svelte";
import "./styles/theme.css";
import "./styles/app.css";
import { pickApi } from "./api";
import App from "./components/App.svelte";

// Desktop dev hook (IG_THEME): a fixed theme for screenshots taken without input.
const theme = new URLSearchParams(location.search).get("theme");
if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;

const api = pickApi();
const target = document.getElementById("app")!;
api.viewData().then(
  (data) => mount(App, { target, props: { data, api } }),
  (e) => (target.textContent = "ig: could not load the data: " + String(e?.message ?? e)),
);
