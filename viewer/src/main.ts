// Boots the viewer: picks how to reach the data (inline JSON, `ig serve`, or the desktop app), reads it
// once and mounts <App>, which follows the host's rebuilds from then on.

import { mount } from "svelte";
import "./styles/theme.css";
import "./styles/app.css";
import { pickApi } from "./api";
import App from "./components/App.svelte";

const api = pickApi();
api.viewData().then((data) => mount(App, { target: document.getElementById("app")!, props: { data, api } }));
