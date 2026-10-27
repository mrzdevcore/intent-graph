// Boots the viewer: picks the host (the desktop app or none) and mounts <App>.

import { mount } from "svelte";
import "./styles/theme.css";
import "./styles/app.css";
import { pickApi } from "./api";
import App from "./components/App.svelte";

// Desktop dev hook (IG_THEME): a fixed theme for screenshots taken without input.
const theme = new URLSearchParams(location.search).get("theme");
if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;

mount(App, { target: document.getElementById("app")!, props: { api: pickApi() } });
