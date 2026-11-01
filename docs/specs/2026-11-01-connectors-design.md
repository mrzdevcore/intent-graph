# Connectors: what is running, on the map

## Why

ig reads code and Ansible, so its map says what a system should be. Nothing says what is
running. Someone with a server on their network wants one map that answers both: "which services
do I have, are they up, what routes to them, and what runs that no repo describes?"

Incus comes first because it is the server in front of us. Kubernetes and others will follow, so
Incus must be the first connector rather than a special case. Being able to manage a server, and
to open a box's logs and monitoring, comes later: the design leaves room and builds neither.

## Goals

- `ig view` shows a live Incus server: its instances, their state and the routes reaching them.
- One service is one box: an Ansible role and the instance it deploys are the same box.
- A running instance that no role or box describes is visible and flagged, not hidden.
- Opening a box shows something: its routes inside, and a pad on its edge for each connection.
- A new connector is a new crate plus one line in `ig-wire`, with no change to the map code.

## Non-goals

- Starting, stopping or changing anything. Connectors only read.
- Logs, metrics, polling or a live-updating page. State is as of the last build.
- A Kubernetes connector. Its shape is checked on paper only (see "The port").
- Reading what runs inside an instance (Docker apps, k3s workloads).
- Connectors in the desktop app. It would need its own trust prompt, and `ig.yaml`'s connector entries
  would have to count towards trust. Its map draws code and Ansible as today, with one notice when
  `ig.yaml` has connectors.
- Reading Traefik's deployed file over SSH.

## Anti-goals: a failure even if it works

- **A secret reaches ig's output.** Instance config, environment variables and Docker env must
  never be parsed, stored, printed, or put in `view.html` or `ig/`.
- **An unreachable server breaks ig.** It is a notice, never an error, and the rest of the map
  still draws.
- **`ig check` depends on a server.** CI gives the same answer whether or not the server is up.
- **A repo makes ig run a command it chose.** See "Security".
- **A wrong match.** Showing one service's state on another's box is worse than showing none, so
  there is no fuzzy matching.

## Constraints

- `ig-core` is pure domain plus ports; adapters depend only on `ig-core`; `ig-wire` connects them.
  `crates/ig/tests/architecture.rs` enforces this and a 600-line file limit, and counts the `ig-*`
  crates (bump its count). `crates/ig-app/src/map/ansible.rs` is at 594 lines: new Ansible code
  goes elsewhere.
- `ig/` changes only by a person's or agent's edit. A connector never writes to it.
- Tests use fixture JSON and need no server. Fixtures use made-up names and addresses: this repo is
  public, so no real host, domain or IP goes in code, tests or docs.
- Viewer: colours in `themes/*.css`, sizes from `tokens.css`, rem not px.

## Decided by the user

- Incus first; Kubernetes or others later, so a connector seam, not an Incus mode.
- Read-only for now. Logs and monitoring from a box come later.
- An Ansible role and its instance are **one box**.
- Inside a box: the "routes inside Traefik" mockup, board `PadsRoutes` on the
  [wiring canvas](the design canvas): routes as boxes on the left,
  a wire from each to a labelled pad on the box's edge, a stopped target shown as a grey dotted wire
  with a "stopped" chip. Clicking a pad goes to the box at the other end.
- Security rule above: allowlist the fields parsed; never read config or environment.
- Traefik's routes come from a config file in the repo, not from the server. ig needs no access to
  the server beyond the `incus` client. A stale file shows as warnings on its routes.
- Connectors run in the browser `ig view` only in this version, not in the desktop app.

## The design

### Configuration

```yaml
# ig.yaml
connectors:
  - { kind: incus, remote: homelab }
  - { kind: traefik, file: traefik-dynamic.yml }
```

An unknown `kind` or a missing key is a notice naming the entry, and that entry is skipped. No
`connectors` key means nothing changes for anyone.

### The port

In `ig-core::ports`, beside `LanguageFrontend`, and listed in `ig_wire::CONNECTORS`:

```rust
pub trait Connector: Send + Sync {
    /// The `kind` it answers to in ig.yaml.
    fn kind(&self) -> &'static str;
    /// What is there now. Err is shown as a notice; ig carries on.
    fn read(&self, settings: &Settings, files: &dyn Files) -> Result<Snapshot, PortError>;
}

pub struct Snapshot { pub resources: Vec<Resource>, pub links: Vec<Link> }

pub struct Resource {
    /// Stable: (connector kind, remote, name). Logs and metrics attach to this later.
    pub id: ResourceId,
    pub title: String,
    pub kind: ResourceKind,   // Service, Route
    pub parent: Option<ResourceId>,
    pub state: State,         // Running, Stopped, Unknown
    /// How other connectors find it. Used for matching, never drawn.
    pub addresses: Vec<String>,
}

pub struct Link { pub from: ResourceId, pub to: Target, pub label: Option<String> }
pub enum Target {
    Resource(ResourceId),
    /// The address it was given, and the names it goes by. Tried in that order, exact only.
    Lookup { address: Option<String>, names: Vec<String> },
}
```

`Settings` holds validated strings only. A snapshot has nowhere to put config or environment: the
security rule is held by the type, not by care.

Checked on paper against Kubernetes, not built: a pod or service is a `Resource` with a namespace
as `parent`, an Ingress rule a `Route`, a Service's selector a `Link`.

### Incus connector (`ig-conn-incus`)

Runs `incus list <remote>: --format json` with a 5 second deadline, without a shell. It parses into
a struct holding only `name`, `type`, `status` and `state.network.<iface>.addresses[].{address,
scope}`. Everything else, `config`, `expanded_config`, `devices` and `expanded_devices` included, is
never deserialized. A failed parse reports "could not read the instance list", never the output.

Status `Running` and `Stopped` map to `State`; anything else is `Unknown`.

Addresses: global-scope addresses only, leaving out interfaces named `lo`, `docker*`, `br-*`, `cni*`,
`flannel*` and `veth*`. This matters: instances running Docker all report the same `172.17.0.1`, so
matching on those would connect a route to the wrong instance. A fixture must cover this.

A stopped instance reports `state.network: null`, so it has no addresses (checked against a real
server, where no instance had a static address in its devices either). This is why a link also
carries names: see "Matching resources to boxes".

### Traefik connector (`ig-conn-traefik`)

Reads a dynamic config file through `Files`, with `serde_yaml_ng`, keeping only router names, their
`Host(...)` rules, their service, and that service's server URLs. Middlewares and anything else are
not parsed.

- One `Service` resource, named `traefik`, holds a `Route` resource per router, titled by its host.
  Its id is `(traefik, <file>, traefik)` and its state is `Unknown`: a file cannot say whether Traefik
  is up.
- Each route has a `Link` to a `Lookup`: the address is the server URL's host when that is an IP; the
  names are that host when it is a name, then the router's name.
- Routers with an internal service (`api@internal`) are left out.

Traefik is its own connector because routes are not an Incus idea, and it could as well run on k3s.
It also proves the port with a second implementation.

### Matching resources to boxes

New pure code in `crates/ig-app/src/map/live.rs`, run after `compare` and before `view`:

1. **An explicit link wins.** A new common box property in `ig/*.kdl`, which may repeat when one box
   is several instances: `runs="incus:homelab/gitlab"`. The spelling is `<kind>:<remote>/<name>`.
2. **Otherwise exact name:** a `Service` resource's name equals the last part of a box's id
   (`services/gitlab` matches `gitlab`). More than one candidate is no match, plus a warning naming
   them.
3. **Otherwise it is unmanaged:** drawn in a group named after its remote, with origin
   `Unmanaged`, not in `ig/`, chip "not in ig/".

| Box in `ig/` or a role | Resource | On the map |
|---|---|---|
| Yes | Yes | One box with a `state` mark: running is ok, stopped is down |
| Yes, and its name matches no resource | No | No mark: most boxes are not servers |
| Yes, with `runs=` | No | A warn mark, "not found on homelab" |
| No | Yes | An unmanaged box |

A box with several resources shows the worst state, with a count in its value: "1 of 2 stopped".
`Unknown` gives no mark and never outranks another state.

**A failure is per connector.** What failed gives one notice, and nothing is concluded from its
absence: its `runs=` boxes get no "not found", and it draws no unmanaged boxes. Other connectors still
draw. While any connector failed, a route that resolves to nothing gets no warning, since the target
may be on the one that failed. A failed read says nothing about what runs there.

**Traefik's box.** The `traefik` resource does not take part in the name rule against other resources.
Its routes go inside, in order: a box matching `traefik` by the rules above; else an instance named
`traefik` from any connector, whose state then applies; else a box of its own at the top level.

**Routes.** Routes use the existing `entry` kind with `kind=route`. A `Lookup` resolves against every
snapshot: the address against `addresses`, then each name against resource names, exact only. A stopped
target therefore resolves by name, and its pad shows "stopped". A route resolving to nothing gets a
warn mark, "no instance at this address", which also shows a stale config file.

**Not one to one.** A role deployed to several instances (`k3s-agent` and instances `k3s-agent-1`,
`k3s-agent-2`) does not match by name: its instances show as unmanaged until `runs=` links them. Reading
inventory groups could do this later. An unfoldered role rolled up into a box of several has no box of
its own, so its instance shows as unmanaged: accepted.

### What this does to the rest of ig

- **"To build" (the brief's fix 2).** Today a box with no `at` is already built, but an arrow touching
  one is "to build", which is wrong for something running. `arrows_against` treats an arrow touching a
  box with a `runs=` property like one touching a person: a declaration, built. This reads only the
  spec, so `ig plan`, `ig check` and `ig view` agree, with the server up or down, and nothing changes in
  `compare`'s signature or for existing repos. A box matched by name alone gets no such treatment.
- **Order.** `compare` runs as today, and `live.rs` runs after it: it adds marks, unmanaged boxes and
  route arrows (always built), and never re-judges an arrow.
- **State is an in-memory layer**, named `state`, added to `View.marks`. No file under `.ig/layers`
  is written. `View` gets it through the same path as `rules` and `risks`.
- **Arrows** go through `arrows::lift`, so routes crossing groups keep every route in `parts`.
- **Where connectors run.** `view::run_with` gains a list of connectors: `ig view` passes `CONNECTORS`;
  the desktop app, `ig plan`, `ig check`, `ig accept` and MCP's plan pass none, so those give the same
  answer with the server off.
- **`ig accept` does not cover unmanaged boxes** in this version. To bring one in, a person adds a box
  in `ig/`, and the name rule or `runs=` links it.

### The viewer

Mockup: board `PadsRoutes`. Show the board's shape, and use the theme tokens rather than its literal
colours.

- **Pads.** When a box is open, for every arrow drawn at any level above it that has an end in the box,
  draw a pad on the box's edge, labelled by the title at the other end. Clicking it opens that box.
- With `parts`, a wire runs from the inner box that holds the part's end to its pad.
- With no `parts` (a rollup from code edges), one pad per arrow, with a short stub.
- A pad whose target has a `down` state mark is grey, dotted, with a "stopped" chip.
- `viewer/src/map/wires.ts` routes within one parent today, and pads need a route from a child to the
  parent's own edge. Pads are per open box and only drawn when it is open.
- **The card.** "Links out" and `×N` count code edges, so declared arrows read 0 under "Uses · 3".
  Count a declared arrow as 1.
- Clicking a box that has nothing inside and no arrows keeps today's behaviour.

## Security

- Remote and path come from `ig.yaml`, which a cloned repo controls. `remote` must match
  `[A-Za-z0-9_-]+` and is passed as one argument, never through a shell. `ig.yaml` cannot name a
  program or a command.
- `file` must be relative with no `..`, and is read through `Files`.
- The desktop app runs no connectors, so its trust model is unchanged.
- A repo can still make the CLI list a remote the person has configured. Nothing leaves the machine
  and the result goes to the gitignored `.ig/view.html`, so this is accepted; it is called out here
  so that it can be revisited.
- Output: instance names, states, route hosts and titles reach `view.html`. Addresses do not.
- Never print `incus` stdout. Its stderr may be shown, first line only.

## Tests

- Parsing from fixture JSON that **includes** `config`, `expanded_config`, `devices` and a secret-looking
  value: assert the value appears nowhere in the `Snapshot`'s debug output or the final `view.html`.
- The shared `172.17.0.1` case: a route does not match the instance it merely shares a Docker bridge with.
- Each row of the matching table, the ambiguous name, `runs=` repeated, and worst-state on one box.
- A route to a stopped instance (`network: null`) resolves by name and is not reported as stale.
- `traefik` as a box, as an instance, and as neither.
- One connector returning Err: a notice, the map draws, its own results absent, the other's kept, no
  "not found" and no stale-route warnings.
- `ig plan`, `ig check` and the desktop's view do not call a connector (a fake that fails the test if
  called).
- A bad `remote` is refused before anything runs.
- Viewer: pad placement from `parts` as a pure function, in the style of `map-wires.test.ts`.
- A manual run against a real server, with listing only. Never start, stop or delete anything.

## Left to the builder

- The names and layout of the new crates and of `live.rs`, keeping files under 600 lines.
- How the `incus` runner is shared or faked, and the exact text of notices.
- Where pads sit along an edge, and how they avoid each other, using the mockup as the guide.
- Build order. Suggested: port and Incus with fake data, then matching and the state layer, then
  Traefik, then pads last, since they are useful without any connector.

## Calls made in writing this, for review

1. A Traefik file is read through `Files`, so it must sit inside the repo.
2. `ig plan`, `ig check` and `ig accept` never contact a server.
3. Matching is on the last part of the box id, not its title.
4. Unmanaged boxes sit in a group named after the remote.
5. A box with a name that matches nothing gets no mark, since most boxes are not servers.
6. State lives in memory, not in `.ig/layers`.
7. A repo's `ig.yaml` can make the CLI list a configured remote without asking.
8. A route falls back to an exact name, including its router's name, because a stopped instance has
   no address.
9. `runs=` may repeat; a box shows its worst state. A role on several instances needs `runs=`.
10. A connector's failure affects only its own results, and stale-route warnings pause meanwhile.
11. An arrow touching a `runs=` box counts as built everywhere; matching by name does not do this.
12. Traefik's routes go inside a `traefik` box, then a `traefik` instance, then a box of their own.
