---
name: draw
description: Draw or change the intended architecture in ig/ - add, move, rename or remove boxes and arrows, sketch a target architecture before the code exists. Use when asked to design the architecture, draw the target, add a service or store to the map, or change ig/.
---

# Draw the intended architecture in ig/

`ig/` is the team's intended architecture. Without an `ig/`, tell the user to run `ig analyse`;
never run it yourself.

1. Change `ig/` through the `ig_edit` tool (`{"edits": [...]}`: add, remove, move, rename, set,
   arrow, unarrow). Your edits are proposals the user reviews under Proposals in the map. Check
   the result with `ig_plan`.
2. When the `ig_*` tools are not available, say so first: in Claude Code they appear once the
   folder is trusted and `/reload-plugins` has run. Edit `ig/*.kdl` yourself only when the user
   agrees; then run `ig fmt` and `ig plan`, and fix every error and warning they print.
3. Keep each box to at most 12 boxes inside it, and give a box that has code its `at` globs.

The format of `ig/` follows; it is also `man 5 ig-map`.

# ig-map - the intended architecture in ig/*.kdl

## SYNOPSIS

ig/system.kdl, ig/*.kdl, ig/kinds/*.kdl

## DESCRIPTION

The `ig/` folder holds a project's intended architecture: boxes that nest, and arrows between them. It is KDL (https://kdl.dev), committed with the code and read by every ig command. `ig analyse` writes a first one from the code; after that a person edits it by hand or in the app, and an agent through the `ig_edit` tool. `ig fmt` rewrites the files in ig's own layout, and `ig plan` reports every error with its file and line.

## FILES

- `ig/system.kdl` holds the one `system` node of the whole map. Every other box sits inside it.
- Every other `ig/*.kdl` holds exactly one top-level node, which sits directly in the system. ig names the file after the box's id (`api.kdl`, a `/` in the id becoming `-`); the name is not read.
- `ig/kinds/<name>.kdl` defines a team kind (see TEAM KINDS).

## BOXES

A box is a node: its kind, then its id, then its properties, then a body in braces.

```
service "api" title="API" tech="Rust · axum" description="Answers the web app" {
    at "crates/api/**"
    calls "billing"
    module "api/auth" title="Auth" { at "crates/api/src/auth/**" }
}
```

- The id is a slug (a-z, 0-9, `-`, at most 64 characters), or `parent/slug` for a box inside another. Ids are unique across `ig/`.
- `title` is required (at most 60 characters). Every property value is a string.
- The id is the only value before the properties.

## KINDS

Blocks that run sit in the system, or in a group that is not inside one of them:

- `app`: what people open: a web, mobile or desktop app.
- `service`: a process that runs and answers: an API, a server, a worker.
- `job`: work that runs, then stops: a cron task, a batch, a migration.
- `store`: where data is kept: a database, a bucket, a cache.
- `queue`: where messages wait: a queue, a topic, a stream.
- `model`: an AI model the system calls, hosted or local.
- `external`: a system outside this one, run by someone else. Not code: it takes no `at`.
- `person`: someone who uses the system. Not code: it takes no `at`.

The design of a block sits inside a block that runs, inside another design box, or in a group there:

- `module`: a part of a block's code with one purpose. A module may also sit where blocks that run sit: a library package.
- `entry`: a way in: a route, a command, a schedule, a message handler.
- `usecase`: one thing the block does for someone.
- `domain`: the core: the rules and the words of the business.
- `port`: what the core needs from outside, as an interface.
- `adapter`: what fills a port: the code that talks to the outside.
- `agent`: an AI agent: a prompt, a model and the tools it may use.

And:

- `system`: the whole system this map draws; exactly one, in `ig/system.kdl`.
- `box`: a box whose kind is not decided yet. It sits anywhere a block that runs or a design block may, and holds either; it is never warned about as misplaced. Takes `at`; `ig plan` lists it as to build, kind not decided.
- `group`: boxes kept together, with nothing of its own. It sits anywhere. Not code: it takes no `at`.

A box in a place its kind does not sit in is a warning, not an error.

## PROPERTIES

Every kind takes:

- `title`: its name on the map (required, at most 60 characters).
- `description`: one or two sentences (at most 200 characters).
- `tech`: what it is built with, like `Rust · axum` (at most 60 characters).
- `logo`: a Simple Icons slug, like `postgresql`.
- `owner`: who owns it.
- `slot`: where it sits in its parent's `pattern` (see BODY).
- `runs`: what it runs as, for the connectors: `<connector>:<remote>/<name>`, separated by spaces, like `incus:homelab/web`.

Some kinds take more:

- `model`: `provider` (anthropic, openai, ollama…), `model` (its name), `hosted` (`local` or `hosted`).
- `entry`: `kind` (`route`, `command`, `schedule` or `message`).
- `agent`: `prompt` (its instructions, or the file that holds them).

Any other property is an error.

## BODY

The body of a box holds, in any order:

- `at "<glob>"…`: the code the box is made of, globs relative to the repository root, without `..`; `*` stays within a folder, `**` crosses folders. A box without `at` is drawn but tied to no code.
- `badges "<text>"…`: at most 8 short labels (at most 40 characters each).
- `pattern "<name>"`: how the box's inside is drawn: `onion` (slots `domain`, `application`, `infrastructure`), `hexagonal` (slots `core`, `ports`, `adapters`), `layered "<band>"…` (one slot per band, top first) or `none`. A box inside it puts itself in a slot with `slot=`.
- `context "<text>"`: free text of any length for whoever builds the box; `description` stays the one line shown on hover. With line breaks, write a raw multi-line string: `context #"""`, the text indented, then `"""#` on its own line, as deep as the text.
- Arrows, from this box (see ARROWS).
- The boxes inside it.

`at`, `badges`, `pattern` and `context` are each written once.

## ARROWS

An arrow is written in the box it leaves: its kind, the id it points to, and the optional `label`, `note`, `context` and `shape`:

```
calls "billing" label="charges the card" note="once per checkout"
```

- `label`: what it carries or does, a few words.
- `note`: one short line shown on the map under the label, at most 80 characters, no line break (more is an error).
- `context`: free text of any length for whoever builds it.
- `shape`: how the arrow is drawn on the map, one of `circuit`, `elbow`, `curved` or `straight`; without it the map chooses (any other value is an error).
- Any other property on an arrow is a warning; the arrow is still read.

- `calls`: asks it something and waits for the answer.
- `sends`: puts a message on a queue. It goes to a `queue`.
- `reads`: reads data from it. It goes to a `store` or a `queue`.
- `writes`: changes data in it. It goes to a `store`.
- `triggers`: starts it. It goes to a `job` or an `entry`.
- `implements`: fills a port. It goes from an `adapter` to a `port`.
- `uses`: depends on its code.

- One arrow per pair of boxes. When several kinds hold, write the first that does in the order above (calls, sends, reads, writes, triggers, implements, uses), and say the rest in `label`, like `writes "db" label="reads and writes"`.
- The target exists somewhere in `ig/`; an arrow never points at its own box.
- An arrow whose ends break the rules above is a warning, not an error.
- `bundle` is what the map draws for several kinds of dependency at once; it is never written.

## TEAM KINDS

A team adds a kind in `ig/kinds/<name>.kdl`, extending a built-in one, and uses it like any other:

```
kind "gateway" is="service" title="Gateway" means="Where outside traffic comes in" icon="shield" {
    property "vendor" means="Who makes it" one-of="traefik" "nginx"
    property "zone" means="The network it faces" required=#true
}
```

It takes its base kind's place, properties and icon, and adds its own properties. `required=#true` makes one required; `one-of=` lists the values it may take. It cannot use the name of a built-in kind or of an arrow kind, nor `at`, `badges`, `pattern` or `context`.

## OLD NAMES

These are still read, and `ig fmt` rewrites them: `folder` (group), `component` (module), `api` and `worker` (service), `library` (module); arrows `network` (calls), `event` (sends), `data` (reads), `depends` (uses).

## EXAMPLE

`ig/system.kdl`:

```
system "shop" title="Shop" description="Sells books online"
```

`ig/web.kdl`:

```
app "web" title="Web shop" tech="TypeScript · Vue" {
    at "apps/web/**"
    calls "api"
}
```

`ig/api.kdl`:

```
service "api" title="API" tech="Rust · axum" {
    at "crates/api/**"
    pattern "hexagonal"
    domain "api/orders" title="Orders" slot="core" { at "crates/api/src/orders/**" }
    port "api/payments" title="Payments" slot="ports" { at "crates/api/src/payments.rs" }
    adapter "api/stripe" title="Stripe client" slot="adapters" {
        at "crates/api/src/stripe/**"
        implements "api/payments"
        calls "stripe"
    }
    writes "db" label="reads and writes"
    sends "events"
}
```

`ig/db.kdl`, `ig/events.kdl` and `ig/stripe.kdl`:

```
store "db" title="Database" tech="PostgreSQL" logo="postgresql"
queue "events" title="Order events" tech="NATS"
external "stripe" title="Stripe" logo="stripe"
```

That last block is three files of one line each: one top-level node per file.

## SEE ALSO

ig(1): `ig analyse` writes a first `ig/`, `ig plan` checks it, `ig apply` builds what it lists, `ig fmt` lays it out, `ig view` draws it.
