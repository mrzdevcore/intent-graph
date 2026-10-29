// Boots the viewer: picks the host (the desktop app or none) and mounts <App>.

import { mount } from "svelte";
import "./styles/tokens.css";
import "./styles/themes/light.css";
import "./styles/themes/dark.css";
import "./styles/app.css";
import { pickApi } from "./api";
import App from "./components/App.svelte";

// The theme: ?theme=<name> pins one (the desktop dev hook IG_THEME uses it for screenshots), else the
// system's light or dark, followed live. A name with no theme file falls back to light.
const root = document.documentElement;
const pinned = new URLSearchParams(location.search).get("theme");
const dark = matchMedia("(prefers-color-scheme: dark)");
const follow = () => (root.dataset.theme = pinned && /^[a-z-]+$/.test(pinned) ? pinned : dark.matches ? "dark" : "light");
follow();
dark.addEventListener("change", follow);

mount(App, { target: document.getElementById("app")!, props: { api: pickApi() } });
