//! Tool definitions for `ig_plan` and `ig_edit`; the `ig_edit` schema follows `Edit`'s serde shape.

use ig_app::map::arrows::TELLING;
use ig_app::map::kinds::Kinds;
use serde_json::{Value, json};

/// The kinds a new box may be, from the table (the team's in `ig/kinds/` among them): `system`
/// is left out (one root, already written); the code-level kinds are not in it.
pub fn kinds(k: &Kinds) -> Vec<String> {
    let k = k.names().into_iter();
    k.filter(|k| *k != "system").map(String::from).collect()
}

/// The arrow kinds written; `bundle` is derived, never written.
fn arrows() -> Vec<Value> {
    TELLING.iter().map(|k| json!(k)).collect()
}

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

/// The tools, with the kinds of the project's table.
pub fn tools(k: &Kinds) -> Value {
    let mut one = json!({"oneOf": [
        op("add", json!({
            "parent": s("id of the box it goes in"),
            "kind": {"type": "string", "enum": kinds(k)},
            "id": s("lowercase slug, unique"),
            "title": s("shown name"),
            "at": {"type": "array", "items": {"type": "string"}, "description": "code globs the box owns; always give it for a new box"},
            "before": s("sibling id to go before (default: last)"),
            "props": {"type": "object", "additionalProperties": {"type": "string"}, "description": "the kind's own properties, like provider, model and hosted (local|hosted) for a model, or prompt for an agent"}
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
            "key": s("title, tech, description, logo, slot, owner, or a property of the box's kind (provider, model, hosted for a model; prompt for an agent; a team kind's own); at takes globs joined by newlines; kind renames the box's kind in place (refused where it would put the box out of place; drops the properties the new kind does not take); context is the box's longer text for the builder; prop:<name> sets the kind's own property <name> even when it is named like one of these keys"),
            "value": {"type": ["string", "null"], "description": "null removes the key"}
        }), &["id", "key"]),
        op("arrow", json!({
            "from": s("box id"), "to": s("box id"),
            "kind": {"type": "string", "enum": arrows()},
            "label": {"type": ["string", "null"]}
        }), &["from", "to", "kind"]),
        op("unarrow", json!({
            "from": s("box id"), "to": s("box id"),
            "kind": {"type": "string", "enum": arrows()}
        }), &["from", "to", "kind"]),
        op("arrowset", json!({
            "from": s("box id the arrow starts at"), "to": s("box id the arrow points to"),
            "key": {"type": "string", "enum": ["kind", "label", "note", "context", "shape", "from", "to", "flip"], "description": "kind changes the arrow's kind; label, note (80 characters at most) and context its text; shape is how the arrow is drawn: circuit, elbow, curved or straight; from and to reconnect one end to the box id in value; flip swaps the ends"},
            "value": {"type": ["string", "null"], "description": "null removes label, note, context or shape; flip takes none"}
        }), &["from", "to", "key"]),
    ]});
    let batch = op(
        "batch",
        json!({"edits": {"type": "array", "minItems": 1, "items": one.clone(), "description": "applied in order as one edit: all or nothing, one undo; only the end state has to be valid"}}),
        &["edits"],
    );
    one["oneOf"].as_array_mut().expect("oneOf").push(batch);
    json!([
        {
            "name": "ig_state",
            "description": "What the ig viewer is showing: the last answer written with ig_show (title, nodes, edges, steps), or none.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "ig_show",
            "description": "Draw an answer on the ig map: its boxes and arrows lit, the rest faded, with ordered steps the user walks. Use it whenever the user asks to show something, how something works or where it is used. Ids: boxes[].id and arrows[].from/to of `ig view --json`, or code graph node ids; every id is checked. Writes only .ig/show.json.",
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
            "description": "Edit the system map in ig/ (add, remove, move, rename, set, arrow, unarrow, arrowset, batch). Your edits are proposals the user reviews under Proposals in the map (and with `git diff ig/` in a git repository). Always give `at` (code globs) for a new box. Ids are lowercase slugs. To move a box use `move` (it keeps its id), not remove plus add. To change an arrow (its kind, label, note, context, shape, an end, or its direction) use `arrowset`, not unarrow plus arrow. To change a box's kind use `set` with key `kind`: an `app` no person uses that serves routes is most likely a `service`; propose `set kind service` for it. Use `batch` ({\"op\": \"batch\", \"edits\": [...]}) for edits that belong together, like a new box and an arrow to it: all or nothing, one journal entry, one undo, and only the end state has to be valid. Pass {\"edits\": [...]}: applied in order, stopping at the first error (the ones before it stay recorded). Returns the journal numbers and the plan after the edits.",
            "inputSchema": {
                "type": "object",
                "properties": {"edits": {"type": "array", "minItems": 1, "items": one}},
                "required": ["edits"]
            }
        }
    ])
}
