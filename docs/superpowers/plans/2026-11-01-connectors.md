# Connectors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ig view` draws a live Incus server and its Traefik routes on the same map as the code and Ansible, with one box per service, and any open box shows a pad on its edge for each connection.

**Architecture:** A `Connector` port in `ig-core`, one adapter crate per connector listed in `ig_wire::CONNECTORS`. Pure code in `ig-app/src/map/live.rs` merges their snapshots into the `MapFile` after `compare` and before `view`. Only `ig view` passes connectors; every other command passes none. The viewer gains pads, computed by a pure function.

**Tech Stack:** Rust 2024 workspace, `serde_json`, `serde_yaml_ng`, Svelte 5, vitest.

**Spec:** `docs/superpowers/specs/2026-11-01-connectors-design.md`

## Global Constraints

- Never parse, store, print or put in `view.html` or `ig/`: instance `config`, `expanded_config`, `devices`, `expanded_devices`, environment variables, Docker env.
- A connector failing is a notice, never an error; the rest of the map still draws.
- `ig plan`, `ig check`, `ig accept`, MCP and the desktop app never call a connector.
- `ig.yaml` can never name a program or command. `remote` matches `[A-Za-z0-9][A-Za-z0-9_-]*`. `file` is relative with no `..`.
- A connector never writes to `ig/`. No fuzzy matching: names are exact.
- Adapters depend only on `ig-core`; every file stays under 600 lines (`crates/ig/tests/architecture.rs`). `crates/ig-app/src/map/ansible.rs` is at 594, `origin.rs` at 572: add nothing substantial there.
- This repo is public: tests, fixtures and docs use made-up names and addresses only (`homelab`, `10.0.0.x`, `example.test`).
- Viewer: colours in `viewer/src/styles/themes/*.css`, sizes from `tokens.css`, rem not px.
- Live testing: list instances only. Never start, stop or delete anything.
- Commits: no Claude attribution. Never push without asking.

## Review Focus

1. The `incus` client is not installed, the remote is not configured, or the server hangs: one notice within the 5 second deadline, and the map draws (Task 2).
2. A Traefik file with a missing path, broken YAML, several servers on one service, or a URL with no scheme: a notice or a sensible route, never a panic (Task 3).
3. A hand-written `runs=` that is misspelled: `ig/` reads with a clear error naming the box, and does not silently match nothing (Task 4).
4. An unmanaged group's id colliding with an existing box id, or two remotes both holding an instance called `gitlab`: a unique id, and an ambiguity warning with no match (Task 4).
5. A box with 40 connections: pads stay separate and readable, never stacked on one spot (Task 7).

---

### Task 1: The port and the config

**Files:**
- Modify: `crates/ig-core/src/ports.rs`, `crates/ig-core/src/config.rs`

**Interfaces:**
- Produces, in `ig_core::ports`:
  - `pub type Settings = BTreeMap<String, String>;`
  - `#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)] pub struct ResourceId { pub connector: String, pub remote: String, pub name: String }`
  - `pub enum ResourceKind { Service, Route }`, `pub enum State { Running, Stopped, Unknown }`
  - `pub struct Resource { pub id: ResourceId, pub title: String, pub kind: ResourceKind, pub parent: Option<ResourceId>, pub state: State, pub addresses: Vec<String> }`
  - `pub enum Target { Resource(ResourceId), Lookup { address: Option<String>, names: Vec<String> } }`
  - `pub struct Link { pub from: ResourceId, pub to: Target, pub label: Option<String> }`
  - `#[derive(Default)] pub struct Snapshot { pub resources: Vec<Resource>, pub links: Vec<Link> }`
  - `pub trait Connector: Send + Sync { fn kind(&self) -> &'static str; fn read(&self, settings: &Settings, files: &dyn Files) -> Result<Snapshot, PortError>; }`
  - All with `Debug, Clone, PartialEq` where they hold data.
- Produces, in `ig_core::config`: `pub struct ConnectorEntry { pub kind: String, pub settings: BTreeMap<String, String> }` and `Config.connectors: Vec<ConnectorEntry>`.

- [ ] **Step 1: Write the failing test** in `config.rs`'s tests

```rust
#[test]
fn connectors_are_read_as_a_list_of_kinds_and_settings() {
    let c = Config::parse("connectors:\n  - { kind: incus, remote: homelab }\n  - { kind: traefik, file: t.yml }\n").unwrap();
    assert_eq!(c.connectors[0].kind, "incus");
    assert_eq!(c.connectors[0].settings["remote"], "homelab");
    assert_eq!(c.connectors[1].settings["file"], "t.yml");
    assert!(Config::parse("connectors: nope\n").unwrap_err().0.contains("connectors"));
    assert!(Config::parse("connectors:\n  - { remote: homelab }\n").unwrap_err().0.contains("kind"));
    assert!(Config::default().connectors.is_empty());
}
```

- [ ] **Step 2: Run it:** `cargo test -p ig-core connectors_are_read`. Expected: FAIL, unknown key or no field.
- [ ] **Step 3: Implement.** Add the port types and the `"connectors"` arm in `Config::parse` (around `config.rs:199`). Each entry is a mapping of string values, `kind` required. An unknown `kind` is not an error here: `live::read_all` reports it.
- [ ] **Step 4: Run it:** `cargo test -p ig-core`. Expected: PASS.
- [ ] **Step 5: Commit:** `git add crates/ig-core && git commit -m "Connector port and its config"`

---

### Task 2: The Incus connector

**Files:**
- Create: `crates/ig-conn-incus/Cargo.toml`, `crates/ig-conn-incus/src/lib.rs`, `crates/ig-conn-incus/src/tests.rs`
- Modify: `crates/ig/tests/architecture.rs` (`seen` count 7 to 8, and its message)

**Interfaces:**
- Consumes: Task 1's port.
- Produces: `pub struct IncusConnector;` implementing `Connector` with `kind() == "incus"`, and `pub fn parse(remote: &str, json: &str) -> Result<Snapshot, PortError>`.

Resources: `ResourceId { connector: "incus", remote, name }`, `ResourceKind::Service`, title = name.

- [ ] **Step 1: Write the failing tests** with a fixture holding two running instances running Docker, one stopped with `"network": null`, and in each a `config`, `expanded_config` and `devices` containing `"SECRET_CANARY"`.

```rust
#[test] fn only_the_allowlisted_fields_are_kept() {
    let s = parse("homelab", FIXTURE).unwrap();
    assert!(!format!("{s:?}").contains("SECRET_CANARY"));
    assert_eq!(s.resources[0].id.name, "alpha");
    assert_eq!(s.resources[0].state, State::Running);
}
#[test] fn a_stopped_instance_has_no_addresses() { /* state Stopped, addresses empty */ }
#[test] fn docker_bridge_addresses_are_left_out() {
    // alpha and beta both report 172.17.0.1 on docker0 and 172.18.0.1 on br-abc
    let s = parse("homelab", FIXTURE).unwrap();
    assert_eq!(s.resources[0].addresses, ["10.0.0.2"]);
    assert_eq!(s.resources[1].addresses, ["10.0.0.3"]);
}
#[test] fn a_parse_failure_never_echoes_the_output() {
    let e = parse("homelab", "SECRET_CANARY not json").unwrap_err().to_string();
    assert_eq!(e, "incus homelab: could not read the instance list");
}
#[test] fn a_bad_remote_is_refused_before_anything_runs() {
    for bad in ["-x", "a b", "a:b", "", "../x"] {
        let e = IncusConnector.read(&settings(bad), &NoFiles).unwrap_err().to_string();
        assert!(e.contains("remote"), "{bad}: {e}");
    }
}
#[test] fn a_missing_client_is_a_clear_error() { /* run_with("/no/such/incus", ...) names incus */ }
#[test] fn a_hanging_client_is_stopped_at_the_deadline() { /* a script sleeping 30s, deadline 1s: Err within 5s */ }
#[test] fn stderr_shows_its_first_line_only() { /* a script failing with two stderr lines */ }
```

- [ ] **Step 2: Run:** `cargo test -p ig-conn-incus`. Expected: FAIL to compile, then FAIL on each.
- [ ] **Step 3: Implement.** `parse` deserializes into private structs holding only `name`, `type`, `status`, `state.network.<iface>.addresses[].{address, scope}`; `state` and `network` are `Option`. Keep `scope == "global"`, and leave out interfaces named `lo` or starting `docker`, `br-`, `cni`, `flannel`, `veth`. `Running` and `Stopped` map to `State`, anything else `Unknown`. `fn run_with(program: &str, remote: &str, deadline: Duration) -> Result<String, PortError>` runs `<program> list <remote>: --format json` with `std::process::Command`, stdin null, polling `try_wait` as `crates/ig/src/agent.rs:run_argv` does, default 5 seconds. Stdout is never included in an error.
- [ ] **Step 4: Run:** `cargo test -p ig-conn-incus && cargo test -p intent-graph --test architecture`. Expected: PASS.
- [ ] **Step 5: Commit:** `git add crates/ig-conn-incus crates/ig/tests/architecture.rs Cargo.lock && git commit -m "Incus connector: instances, states and addresses, read-only"`

---

### Task 3: The Traefik connector

**Files:**
- Create: `crates/ig-conn-traefik/Cargo.toml`, `crates/ig-conn-traefik/src/lib.rs`
- Modify: `crates/ig/tests/architecture.rs` (count 8 to 9)

**Interfaces:**
- Consumes: Task 1's port; `Files::read`.
- Produces: `pub struct TraefikConnector;`, `kind() == "traefik"`.

The service resource is `ResourceId { connector: "traefik", remote: <file>, name: "traefik" }`, state `Unknown`. Each router is a `ResourceKind::Route` with `name` = the router's name, `title` = its first `Host(...)`, or the router's name when it has none, and `parent` = the service.

- [ ] **Step 1: Write the failing tests** using `ig_core::testing::MemFiles` (dev-dependency with feature `testing`).

```rust
const FILE: &str = r#"
http:
  middlewares:
    auth: { basicAuth: { users: ["SECRET_CANARY"] } }
  routers:
    dash: { rule: "Host(`dash.example.test`)", service: api@internal }
    app:  { rule: "Host(`app.example.test`)", service: app-svc }
    wiki-router: { rule: "Host(`wiki.example.test`) && PathPrefix(`/x`)", service: wiki-svc }
    multi: { rule: "Host(`m.example.test`)", service: multi-svc }
  services:
    app-svc:   { loadBalancer: { servers: [{ url: "http://10.0.0.2:80" }] } }
    wiki-svc:  { loadBalancer: { servers: [{ url: "http://wiki:8080" }] } }
    multi-svc: { loadBalancer: { servers: [{ url: "10.0.0.5:80" }, { url: "http://10.0.0.6:80" }] } }
"#;

#[test] fn routers_become_routes_inside_one_service() {
    let s = read(FILE);
    let titles: Vec<_> = routes(&s).iter().map(|r| r.title.clone()).collect();
    assert_eq!(titles, ["app.example.test", "m.example.test", "wiki.example.test"]); // no api@internal
    assert!(!format!("{s:?}").contains("SECRET_CANARY"));
}
#[test] fn a_link_looks_up_an_ip_then_names() {
    assert_eq!(target(&read(FILE), "app"), Target::Lookup { address: Some("10.0.0.2".into()), names: vec!["app".into()] });
    assert_eq!(target(&read(FILE), "wiki-router"), Target::Lookup { address: None, names: vec!["wiki".into(), "wiki-router".into()] });
}
#[test] fn the_first_server_stands_for_a_service_and_a_missing_scheme_is_fine() {
    assert_eq!(target(&read(FILE), "multi"), Target::Lookup { address: Some("10.0.0.5".into()), names: vec!["multi".into()] });
}
#[test] fn a_bad_path_a_missing_file_and_broken_yaml_are_errors_not_panics() {
    for bad in ["/etc/x", "../x", ""] { assert!(read_at(bad, FILE).unwrap_err().to_string().contains("file")); }
    assert!(read_missing("t.yml").is_err());
    assert!(read_text("{ not: [yaml").is_err());
}
```

- [ ] **Step 2: Run:** `cargo test -p ig-conn-traefik`. Expected: FAIL.
- [ ] **Step 3: Implement.** Private serde structs hold only `http.routers.<name>.{rule, service}` and `http.services.<name>.loadBalancer.servers[].url`. Use `BTreeMap` so order is stable. Find `Host(` + backtick by hand. Parse the host from the URL by hand, with no new dependency: strip a scheme, cut at `:` or `/`; an all-digits-and-dots host is an address, anything else a name. A router whose service contains `@` or is not in `services` is left out. A malformed YAML error says "traefik <file>: could not read it", without quoting the file.
- [ ] **Step 4: Run:** `cargo test -p ig-conn-traefik && cargo test -p intent-graph --test architecture`. Expected: PASS.
- [ ] **Step 5: Commit:** `git add crates/ig-conn-traefik crates/ig/tests/architecture.rs Cargo.lock && git commit -m "Traefik connector: routes from a file in the repo"`

---

### Task 4: `runs=` and matching

**Files:**
- Create: `crates/ig-app/src/map/live.rs`, `crates/ig-app/src/map/live/tests.rs`
- Modify: `crates/ig-app/src/map/mod.rs`, `crates/ig-app/src/map/spec/read.rs:307`, `crates/ig-app/src/map/origin.rs` (`Origin::Unmanaged`, `person` closure at line 249, `plan_of`), `crates/ig-app/src/map/layers.rs` (`mark` becomes `pub(super)`), `crates/ig-app/src/map/edit/set.rs:14` (`COMMON` gains `runs`, so edits can set it)

**Interfaces:**
- Consumes: Task 1's port; `facts::{slug, dedupe}`; `file::fresh`; `layers::mark`.
- Produces:
  - `pub struct ConnectorRead { pub label: String, pub result: Result<Snapshot, String> }`, `label` being what a notice names.
  - `pub fn read_all(config: &Config, connectors: &[&dyn Connector], files: &dyn Files) -> (Vec<ConnectorRead>, Vec<String>)`: one read per `config.connectors` entry; an unknown `kind` gives a notice naming the entry and skips it.
  - `pub fn apply(m: &mut MapFile, reads: &[ConnectorRead]) -> Applied`, with `pub struct Applied { pub notices: Vec<String>, pub warnings: Vec<String> }`.
  - `pub fn parse_runs(value: &str) -> Result<Vec<ResourceId>, String>`: space-separated `<kind>:<remote>/<name>`.
  - `Origin::Unmanaged`, serialized `"unmanaged"`.
  - Layer name `state`; route arrows have `kind: Calls`, `origin: Built`, and their `parent` set to the lowest common parent of their ends, so `arrows::lift` keeps them in `parts`.

- [ ] **Step 1: Write the failing tests** in `live/tests.rs`. Build `MapFile`s by parsing KDL through `spec::parse`.

```rust
#[test] fn a_box_named_like_an_instance_gets_its_state() { /* services/gitlab + stopped gitlab: one box, state mark Down, no new box */ }
#[test] fn an_instance_nothing_describes_is_unmanaged_in_a_group_named_for_its_remote() {
    /* box `homelab/redis`, origin Unmanaged, parent `homelab` whose kind is Group, under the system box */
}
#[test] fn a_box_with_no_instance_and_no_runs_gets_no_mark() {}
#[test] fn runs_finds_its_instance_whatever_the_box_is_called() {}
#[test] fn runs_naming_nothing_warns_on_the_box() { /* warn mark, value "not found on homelab" */ }
#[test] fn runs_listing_several_shows_the_worst_state() { /* Down, value "1 of 2 stopped" */ }
#[test] fn two_boxes_with_one_name_match_neither_and_warn() { /* warning names both; instance stays unmanaged */ }
#[test] fn two_remotes_with_one_name_match_neither_and_warn() {}
#[test] fn an_unmanaged_group_never_takes_an_existing_id() { /* spec has a box `homelab`: group id `homelab-2` */ }
#[test] fn a_failed_connector_concludes_nothing() {
    /* incus Err + traefik Ok: one notice; `runs=` box has no "not found"; no unmanaged; routes draw; a route resolving to nothing has no warn */
}
#[test] fn a_route_resolves_by_address_then_by_name() { /* incl. a stopped instance with no addresses, by router name */ }
#[test] fn a_route_resolving_to_nothing_warns() { /* value "no instance at this address" */ }
#[test] fn routes_sit_in_a_traefik_box_then_a_traefik_instance_then_their_own() {}
#[test] fn route_ends_are_kept_when_they_cross_groups() { /* after arrows::lift, the drawn arrow's parts hold every route */ }
#[test] fn parse_runs_refuses_what_it_cannot_read() {
    assert!(parse_runs("gitlab").is_err());
    assert!(parse_runs("incus:homelab").is_err());
    assert_eq!(parse_runs("incus:a/b incus:a/c").unwrap().len(), 2);
}
```

In `spec/read.rs`'s tests: `runs="nonsense"` gives an error naming the box and the expected spelling, and `runs="incus:a/b"` round-trips through `spec::write`. In `origin/tests.rs`:

```rust
#[test] fn an_arrow_touching_a_box_that_runs_is_a_declaration() {
    /* `service "a" runs="incus:h/a" { calls "b" }`: origin Built, plan empty, no connector involved */
}
```

- [ ] **Step 2: Run:** `cargo test -p ig-app --lib live`, then `cargo test -p ig-app --lib declaration`. Expected: FAIL.
- [ ] **Step 3: Implement `runs`.** In `spec/read.rs:307`, replace the "not yet" error with `parse_runs` validation, storing the value in `b.props`. In `origin.rs`, widen the `person` closure to include `boxes[i].props.contains_key("runs")`, and rename it `declared`. In `plan_of`, treat `Origin::Unmanaged` like `Built`.
- [ ] **Step 4: Implement `apply`.** Order: (1) drop failed reads into notices; (2) match each `Service` resource: `runs=` first, then the last part of a box id, leaving out the Traefik connector's own `traefik` resource; (3) make unmanaged boxes with `file::fresh`, ids `slug(remote)` made unique against every box and child `<group>/<slug(name)>`; (4) add state marks: running `Ok`, stopped `Down`, `Unknown` none, worst across a box's resources; (5) place Traefik's routes per the spec and add them as `BoxKind::Entry` with prop `kind=route`, id `<parent>/route-<slug(name)>`; (6) resolve `Lookup`s, address first then names, and add arrows or warn marks, pausing warnings while any read failed. Addresses are never written to a box.
- [ ] **Step 5: Run:** `cargo test -p ig-app`. Expected: PASS.
- [ ] **Step 6: Commit:** `git add crates/ig-app && git commit -m "Match what is running to the map's boxes; runs= in ig/"`

---

### Task 5: Only `ig view` connects

**Files:**
- Modify: `crates/ig-app/src/map/view.rs`, `crates/ig-wire/src/lib.rs` and `Cargo.toml`, `crates/ig/src/main.rs:61,74`, `crates/ig-desktop/src/commands.rs:22`
- Test: `crates/ig/tests/connectors.rs`

**Interfaces:**
- Consumes: Task 4's `read_all`, `apply`.
- Produces: `pub static CONNECTORS: &[&dyn Connector]` in `ig_wire`, holding both; `pub fn run_live(env: &Env, ask: Option<&mut Ask>, connectors: &[&dyn Connector]) -> crate::Result<View>` in `view.rs`, with `run_with(env, ask)` becoming `run_live(env, ask, &[])`.

- [ ] **Step 1: Write the failing end-to-end tests.** Follow the fake-binary pattern of `crates/ig/tests/agent.rs` (`ig()`, a script on `PATH`). The fake `incus` touches a marker file, then prints a fixture with `SECRET_CANARY` in `expanded_config`.

```rust
#[test] fn view_draws_the_server_and_no_secret_reaches_the_page() {
    let (code, out) = ig(&root, &bin, &["view", "--no-read", "--no-open", "--json"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"state\"") && out.contains("gitlab"));
    assert!(!out.contains("SECRET_CANARY") && !out.contains("10.0.0.2"));
    assert!(!std::fs::read_to_string(root.join(".ig/view.html")).unwrap_or_default().contains("SECRET_CANARY"));
}
#[test] fn plan_and_check_never_call_a_connector() {
    ig(&root, &bin, &["plan"]); ig(&root, &bin, &["check"]);
    assert!(!root.join("called").exists());
}
#[test] fn an_unreachable_server_is_a_notice_and_the_map_draws() { /* fake incus exits 1: code 0, notice, boxes present */ }
#[test] fn a_missing_client_is_a_notice_and_the_map_draws() { /* PATH without incus */ }
#[test] fn an_unknown_connector_kind_is_a_notice() {}
#[test] fn no_connectors_changes_nothing() { /* a repo with no `connectors` key: same view with or without */ }
```

- [ ] **Step 2: Run:** `cargo test -p intent-graph --test connectors`. Expected: FAIL.
- [ ] **Step 3: Implement.** In `run_live`, call `read_all` and `apply` on `m` right after `let mut m = match read { … };` at `view.rs:278`, before `components`. Append its notices to `notices`, and its warnings to `v.warnings` after `warnings(&m, &kinds)`. Both `ig view` paths in `main.rs` call `run_live` with `ig_wire::CONNECTORS`. Add the new crates to `ig-wire`'s `Cargo.toml`; `ig-app` stays free of them.
- [ ] **Step 4: Desktop notice.** In `commands.rs`, when the config has connectors, add one notice to its view: "connectors run in `ig view` only for now". Test it where the desktop's view data is tested.
- [ ] **Step 5: Run:** `cargo test --workspace`. Expected: PASS.
- [ ] **Step 6: Commit:** `git add crates && git commit -m "ig view reads connectors; nothing else does"`

---

### Task 6: State and the card in the viewer

**Files:**
- Modify: `viewer/src/map/types.ts`, `viewer/src/map/looks.ts`, `viewer/src/map/DetailsCard.svelte`
- Test: `viewer/test/map-looks.test.ts`

**Interfaces:**
- Consumes: `origin: "unmanaged"`, marks of layer `state`.
- Produces: `MapArrow.origin` and `MapBox.origin` accept `"unmanaged"`; `export const linkCount = (a: MapArrow) => Math.max(1, a.count)`.

- [ ] **Step 1: Write the failing tests**

```ts
test("an unmanaged box is flagged, in the same family as drift", () => { /* look(...) chip "not in ig/" */ });
test("a stopped box reads as down with the state layer on", () => {});
test("a declared arrow counts as one link", () => { expect(linkCount({ ...a, count: 0 })).toBe(1); expect(linkCount({ ...a, count: 7 })).toBe(7); });
```

- [ ] **Step 2: Run:** `cd viewer && bun run test test/map-looks.test.ts`. Expected: FAIL.
- [ ] **Step 3: Implement.** Use `linkCount` in `DetailsCard.svelte`'s `sum`, `top` and the `×N` row. Show `state` marks in the card's Health list, beside `health`. The Layers menu needs no change: `Map.svelte:133` lists every layer found in the marks, and starts them on. In `looks.ts`, `ORIGIN` gains `unmanaged` with the label "not in ig/".
- [ ] **Step 4: Run:** `cd viewer && bun run test && bun run typecheck`. Expected: PASS, 0 errors.
- [ ] **Step 5: Commit:** `git add viewer && git commit -m "Viewer: state, unmanaged boxes, declared links count"`

---

### Task 7: Pads on a box's edge

**Files:**
- Create: `viewer/src/map/pads.ts`, `viewer/src/map/Pads.svelte`, `viewer/test/map-pads.test.ts`
- Modify: `viewer/src/map/Map.svelte` (in the `.slot` of `level`, around line 437), theme CSS if a new token is needed

**Interfaces:**
- Consumes: `MapArrow.parts`, `View.marks`, `view.placed`.
- Produces:
  - `export type Pad = { side: "in" | "out"; other: string; title: string; child?: string; down: boolean; y: number }`, where `child` is the direct child of the box holding the part's end, and absent for a stub.
  - `export function padsOf(view: View, id: string, frame: { w: number; h: number }, childFrame: (id: string) => Frame | undefined, on: Set<string>): Pad[]`, with `y` in the box's own units.

Mockup: board `PadsRoutes` on the wiring canvas (https://claude.ai/artifact/4Sq16fxPQt4bFzcXAaiSRa). Follow its shape, and use theme tokens rather than its literal colours.

- [ ] **Step 1: Write the failing tests** in `map-pads.test.ts`

```ts
test("each part crossing the edge is a pad, wired from the child holding its end", () => {
  // traefik holds routes r1, r2; the arrow host->public has parts r1->a, r2->b
  expect(padsOf(view, "traefik", F, cf, on).map((p) => [p.side, p.other, p.child])).toEqual([["out", "a", "traefik/r1"], ["out", "b", "traefik/r2"]]);
});
test("a pad sits level with its child", () => {});
test("incoming parts are on the other side", () => {});
test("an arrow with no parts is one pad, with no wire inside", () => {});
test("a target marked down gives a down pad, unless its layer is off", () => {});
test("a box with no connections has no pads", () => {});
test("forty pads stay apart and inside the box", () => {
  const ys = padsOf(crowded, "x", { w: 100, h: 200 }, cf, on).map((p) => p.y);
  expect(new Set(ys).size).toBe(40);
  expect(Math.min(...ys)).toBeGreaterThanOrEqual(0);
  expect(Math.max(...ys)).toBeLessThanOrEqual(200);
});
```

- [ ] **Step 2: Run:** `cd viewer && bun run test test/map-pads.test.ts`. Expected: FAIL.
- [ ] **Step 3: Implement `padsOf`.** A pad comes from every arrow with an end at, or a part's end under, the box, and the other end outside it. Spread pads evenly when level positions would come closer than the pad's height.
- [ ] **Step 4: Implement `Pads.svelte`.** Rendered in the `.slot` when `showsInside(sw)`, for leaf boxes too, in the slot's units; scale text and pads by `f.w / sw` so they keep a steady screen size, as `MapArrow` does with `zoom`. A real `<button>` per pad calls `openBox(other)`. A down pad is grey and dotted with a "stopped" chip. Above 12 pads, labels stay but chips go. When `anchor` has the box covering the whole view its edge is off-screen, so no pads draw: accepted.
- [ ] **Step 5: Check in a browser.** Run `ig view` on a copy of a repo with grouped boxes and compare with the board. Screenshot it for the review.
- [ ] **Step 6: Run:** `cd viewer && bun run test && bun run typecheck`. Expected: PASS, 0 errors.
- [ ] **Step 7: Commit:** `git add viewer && git commit -m "Pads: every connection on an open box's edge"`

---

### Task 8: Docs, ig's own map, and the real server

**Files:**
- Modify: `README.md`, `docs/` config reference if `ig.yaml` keys are listed there, `ig.yaml` (layers for the new crates), `ig/*.kdl` (via `ig accept`)

- [ ] **Step 1: Document** `connectors`, `runs=` and the read-only and secrets rules in `README.md`, in its plain style, with made-up names.
- [ ] **Step 2: Bring ig's own map up to date.** Run `ig plan` here, then `ig accept` its drift for the new crates. Expected: `ig check` exits 0.
- [ ] **Step 3: Run everything:** `make test && cargo clippy --workspace --all-targets`. Expected: all pass, no warnings.
- [ ] **Step 4: Try the real server, listing only.** In a scratch copy of the homelab's `ansible/` and `traefik-dynamic.yml`, with `connectors` in its `ig.yaml`, run `ig view`. Expected: 14 instances accounted for, `gitlab` stopped on its role's box, routes inside `Traefik`, stale routes warned. Search `view.html` for `expanded_config`, `SECRET`, `PASSWORD` and the server's addresses: expect none. Delete the copy after.
- [ ] **Step 5: Commit:** `git add README.md docs ig ig.yaml && git commit -m "Connectors: docs and ig's own map"`
