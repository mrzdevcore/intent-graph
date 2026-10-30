//! Tool definitions for `ig_plan` and `ig_edit`; the `ig_edit` schema follows `Edit`'s serde shape.

use serde_json::{Value, json};

const KINDS: [&str; 12] = [
    "person", "system", "external", "app", "api", "worker", "store", "queue", "library", "entry",
    "usecase", "port",
];
const ARROWS: [&str; 8] = [
    "uses",
    "depends",
    "calls",
    "network",
    "event",
    "implements",
    "data",
    "bundle",
];

fn op(name: &str, props: Value, required: &[&str]) -> Value {
    let mut p = props;
    p["op"] = json!({"const": name});
    let mut req = vec!["op"];
    req.extend(required);
    json!({"type": "object", "properties": p, "required": req, "additionalProperties": false})
}

fn s(d: &str) -> Value {
    json!({"type": "string", "description": d})
}

pub fn tools() -> Value {
    let one = json!({"oneOf": [
        op("add", json!({
            "parent": s("id of the box it goes in"),
            "kind": {"type": "string", "enum": KINDS},
            "id": s("lowercase slug, unique"),
            "title": s("shown name"),
            "at": {"type": "array", "items": {"type": "string"}, "description": "code globs the box owns; always give it for a new box"},
            "before": s("sibling id to go before (default: last)")
        }), &["parent", "kind", "id", "title", "at"]),
        op("remove", json!({"id": s("box id")}), &["id"]),
        op("move", json!({
            "id": s("box id; it keeps its id"),
            "parent": s("new parent id"),
            "before": s("sibling id to go before")
        }), &["id", "parent"]),
        op("rename", json!({"id": s("box id"), "to": s("new lowercase slug")}), &["id", "to"]),
        op("set", json!({
            "id": s("box id"),
            "key": {"type": "string", "enum": ["title", "tech", "description", "logo", "slot", "at"], "description": "at takes globs joined by newlines"},
            "value": {"type": ["string", "null"], "description": "null removes the key"}
        }), &["id", "key"]),
        op("arrow", json!({
            "from": s("box id"), "to": s("box id"),
            "kind": {"type": "string", "enum": ARROWS},
            "label": {"type": ["string", "null"]}
        }), &["from", "to", "kind"]),
        op("unarrow", json!({
            "from": s("box id"), "to": s("box id"),
            "kind": {"type": "string", "enum": ARROWS}
        }), &["from", "to", "kind"]),
    ]});
    json!([
        {
            "name": "ig_state",
            "description": "What the ig viewer is showing: the last answer written with ig_show (title, nodes, edges, steps), or none.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "ig_show",
            "description": "Record an answer for the ig viewer: the nodes and edges of a path or subgraph, highlighted, with ordered steps. Every id is checked against the graph. Writes only .ig/show.json; the system map does not draw it yet (shown in a later version).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "What is shown, like 'How the export reaches the database'"},
                    "nodes": {"type": "array", "items": {"type": "string"}, "maxItems": 60, "description": "Node ids; the first starts a chain"},
                    "edges": {"type": "array", "items": {"anyOf": [
                        {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 2},
                        {"type": "string", "description": "from|kind|to"}]}},
                    "steps": {"type": "array", "maxItems": 40, "items": {"type": "object", "properties": {
                        "target": {"type": "string", "description": "A node id, or from|to for an edge"},
                        "text": {"type": "string"}}, "required": ["target", "text"]}},
                    "mode": {"type": "string", "enum": ["focus", "chain"]}
                },
                "required": ["title", "nodes"]
            }
        },
        {
            "name": "ig_plan",
            "description": "The plan, as `ig plan` prints it: boxes in ig/ still to build, boxes to move (their `at` changed since the last commit, the code is still at the old place), drift (code in no box) and code ig does not read (outside every package). Read-only.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "ig_edit",
            "description": "Edit the system map in ig/ (add, remove, move, rename, set, arrow, unarrow). Your edits are proposals: the user keeps or drops each one. Always give `at` (code globs) for a new box. Ids are lowercase slugs. To move a box use `move` (it keeps its id), not remove plus add. Pass one edit object, or {\"edits\": [...]} applied in order, stopping at the first error. Returns the journal numbers and the plan after the edits.",
            "inputSchema": {"oneOf": [
                one.clone(),
                {"type": "object", "properties": {"edits": {"type": "array", "minItems": 1, "items": one}}, "required": ["edits"], "additionalProperties": false}
            ]}
        }
    ])
}
