# System map: walkthrough

Ten steps, on a scratch copy of a monorepo (never your working copy). Needs bun and cargo; the AI step needs the `claude` CLI logged in.

```sh
SCRATCH=$(mktemp -d)
rsync -a --exclude node_modules /path/to/your/monorepo/ "$SCRATCH/repo"   # keep .git
cd "$SCRATCH/repo"
```

1. **Build.** In the intent-graph checkout: `make build`. Expect `target/release/ig`. Put it on your `PATH` or call it by path below.
2. **Run.** In the scratch repo: `ig view --no-read`. It prints the path of `.ig/view.html` and opens it. Expect a warm run (second time) under 2 seconds, one notice line per removed `ig.yaml` key, and the whole system on screen: the containers, the outside systems, the arrows between them. If the repo's `ig.yaml` has keys from older ig versions (`complexity`, `checkers`, ...), expect one notice line per key, saying it is no longer read.
3. **Fit.** Wheel or pinch to zoom somewhere, drag to pan, then press **Fit** (top right). Expect a glide back to the whole system.
4. **Click into a container.** Click `api` (or your biggest container). Expect a glide into it, the breadcrumb (top left) to read `system > api`, and its components to appear inside, laid out wide, with arrows between them. Click a component to go one level deeper; zoom far enough to see functions.
5. **Details card.** The box you clicked opens a card: description, tech, kind, files, and for each value its source (`ig` or `agent`) and confidence (`extracted`, `inferred`, `ambiguous`). Without a reading, the descriptions are ig's guesses and a line at the bottom says so.
6. **Layers.** Open **Layers** (top right). Untick `rules`, then `risks`: marks and amber arrows disappear; tick them back. Hover or open a marked box to read the mark.
7. **Esc.** With a card open, press **Esc**. Expect the card to close and the keyboard focus to return to the box you opened (Tab moves to the next box).
8. **Drop a health file and re-run.** Create `.ig/layers/health.json` (use real ids from `ig view --json --no-read | head`):
   ```json
   { "layer": "health", "marks": [
     { "target": "api", "state": "warn", "badge": "p95 480 ms" },
     { "target": "no-such-box", "state": "bad" } ] }
   ```
   Run `ig view --no-read` again. Expect a `health` entry in Layers, an amber mark on `api`, and one notice: ``.ig/layers/health.json: no box or arrow `no-such-box`; mark dropped``.
9. **Read with an agent.** `ig view --read --agent claude`. This runs the agent once on every box, with read-only tools, and takes a minute or so. Expect titles and descriptions in plain words, tech and badges, patterns on containers, and a mix of `extracted`, `inferred` and `ambiguous`. Run plain `ig view` in a terminal on a fresh copy to see the question `Let an AI agent (claude) read the architecture? [Y/n]`; answer `n` and the facts-only map opens. `.ig/map.json` now holds the words and is meant to be committed.
10. **First paint and glide speed (browser).** Open `.ig/view.html` in Chrome. Open DevTools > Performance, tick Screenshots, press Reload-and-record (Cmd+Shift+E), stop after the page settles. Expect **First Contentful Paint / LCP under 1 s** in the Timings row. Then record again while you click a container (a glide of about half a second) and while you scroll-zoom for 3 seconds; open the **Frames** row and check the FPS meter (Cmd+Shift+P, "Show frames per second (FPS) meter"). Expect **50 fps or more** throughout, no long red frames. Report the numbers if either misses.
