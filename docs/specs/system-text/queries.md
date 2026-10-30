# Querying and changing ig's model: examples

Sketches for the design discussion, against `shop.kdl`. Nothing here is built.

Two kinds of change, kept apart on purpose:

- **Attributes** (enrichments): data attached to existing nodes, such as telemetry or owners. They
  never change the structure. Each provider writes in its own namespace (`telemetry.*`, `owner.*`).
- **Structure** (boxes and arrows): only through the open change, journaled, undoable, checked
  against the code. Never set directly.

## 1. Selectors (reading)

```css
service                         /* every service */
#api                            /* one node by id */
#api > usecase                  /* use cases directly in the api */
service usecase                 /* use cases anywhere in a service */
[tech=NestJS]                   /* by attribute */
app -> store                    /* apps with an arrow to a store */
app -> * -> store               /* apps two hops from a store */
usecase:has(-> external)        /* use cases calling an outside service */
:planned, :moved, :deleted      /* items of the open change */
service[telemetry.p95-ms > 200] /* slow services (an enrichment) */
store:not(:has(<- service))     /* stores nothing uses (`<-`: incoming arrow) */
```

## 2. The API (like the DOM), for providers and scripts

TypeScript, as a telemetry provider would use it:

```ts
import { open } from "@ig/core";

const sys = await open("./system");

// attach monitoring values from Prometheus to every service
for (const svc of sys.querySelectorAll("service")) {
  const p95 = await prometheus(`histogram_quantile(0.95, http_duration{service="${svc.id}"})`);
  svc.attrs.set("telemetry.p95-ms", Math.round(p95));
  svc.attrs.set("telemetry.errors-per-min", await errorRate(svc.id));
}

// an arrow can carry values too: requests per second from web to api
const call = sys.querySelector("#web").arrowTo("#api");
call?.attrs.set("telemetry.rps", 340);

// read back
sys.querySelectorAll("service[telemetry.p95-ms > 200]").map((n) => n.path);
// → ["/shop/api"]
```

Structure goes through the change, never directly:

```ts
const change = sys.change("Refunds get their own service");
const billing = change.add("#shop", "service", "billing", { tech: "NestJS", at: "billing/**" });
change.move("#api > #refund", billing);          // keeps refund's identity: "moved", not "deleted + new"
change.arrow("#billing > #refund", "calls", "#stripe", { label: "refund" });
change.rename("#payments", "payment-gateway");
change.remove("#api > #refund > calls[to=payments]");
await change.save();                             // journaled; the map shows it dashed until built
```

## 3. Declarative, like a stylesheet

Providers declared once in config; ig refreshes the values. Same selectors, KDL syntax:

```kdl
// system/enrich.kdl
enrich "service" from="prometheus" every="1m" {
    telemetry.p95-ms "histogram_quantile(0.95, http_duration{service=\"{id}\"})"
    telemetry.errors-per-min "sum(rate(http_errors{service=\"{id}\"}[1m]))*60"
}

enrich "store[tech=PostgreSQL]" from="prometheus" every="1m" {
    telemetry.connections "pg_stat_activity_count{db=\"{id}\"}"
}

enrich "*" from="codeowners" {
    owner.team "{owner}"
}
```

And how the map shows them, also by selector:

```kdl
// system/layers.kdl
layer "slow" {
    mark "service[telemetry.p95-ms > 200]" state="warn" badge="p95 {telemetry.p95-ms} ms"
    mark "service[telemetry.errors-per-min > 5]" state="bad" badge="{telemetry.errors-per-min} errors/min"
}
```

## 4. CLI and Claude (MCP): the same verbs

```sh
ig query 'app -> store'                                  # read
ig query 'service[telemetry.p95-ms > 200]' --attrs telemetry
ig set '#api' owner.team=team-orders                     # attributes only
ig change add '#shop' service billing tech=NestJS at='billing/**'
ig change move '#api > #refund' '#billing'
ig change show                                           # the diff, as KDL
```

Claude calls the same through MCP tools (`ig_query`, `ig_set`, `ig_change`), so what it can do is
exactly what the CLI and the map can do.

## The model after the provider ran

```kdl
service api tech="NestJS" at="api/**" owner.team="team-orders" {
    telemetry p95-ms=240 errors-per-min=2   // written by the prometheus provider, refreshed every minute
    ...
}
```

## Where attribute values live (decided with the user: written to files)

- Each provider writes its values to its own file, in the same KDL as the model, keyed by node id,
  with the time they were read: `.ig/attrs/telemetry.kdl`. ig loads them into the model as
  attributes.
- The file holds the latest values; a history file keeps one snapshot per period (hourly by default,
  configurable), enough to see "the api got slow after Tuesday's deploy". (My call.)
- `.ig/attrs/` is not committed by default (values that change every minute would flood git); a team
  commits a snapshot on purpose, at a release for example. (My call.)
- History follows identity: node ids survive renames and moves, so a service's history continues
  after a refactor.

```kdl
// .ig/attrs/telemetry.kdl, written by the prometheus provider
read at="2026-10-30T09:14:00Z"
node api p95-ms=240 errors-per-min=2
node postgres connections=38
arrow from=web to=api rps=340
```
