use super::*;

#[test]
fn protocol_basics() {
    let root = Path::new(".");
    let r = handle(
        root,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    )
    .unwrap();
    let names: Vec<&str> = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["ig_state", "ig_show", "ig_plan", "ig_edit"]);
    let r = handle(
        root,
        &json!({"jsonrpc": "2.0", "id": 3, "method": "initialize", "params": {}}),
    )
    .unwrap();
    assert_eq!(r["result"]["capabilities"], json!({"tools": {}}));
    assert!(
        handle(
            root,
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none()
    );
    let r = handle(root, &json!({"jsonrpc": "2.0", "id": 2, "method": "nope"})).unwrap();
    assert_eq!(r["error"]["code"], -32601);
}

fn call_tool(root: &Path, name: &str, args: Value) -> Value {
    let msg = json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": name, "arguments": args}});
    handle(root, &msg).unwrap()["result"].clone()
}

#[test]
fn writes_show_json_canonically_and_nothing_else() {
    let dir = crate::tests::git_fixture("monorepo");
    let g = head_graph(&dir).unwrap();
    let e = g.edges.iter().find(|e| e.kind == "calls").unwrap();
    let ig_files = |d: &Path| -> Vec<String> {
        std::fs::read_dir(d.join(".ig"))
            .map(|r| {
                r.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let before = ig_files(&dir);
    let args = json!({"title": "How it flows", "nodes": [e.from, e.to], "edges": [format!("{}|{}|{}", e.from, e.kind, e.to)],
        "steps": [{"target": e.from, "text": "starts here"}], "mode": "chain"});
    let r = call_tool(&dir, "ig_show", args.clone());
    assert!(r.get("isError").is_none(), "{r}");
    assert_eq!(
        r["content"][0]["text"],
        "no viewer is open; answer with a diagram too"
    );
    let mut new: Vec<String> = ig_files(&dir)
        .into_iter()
        .filter(|f| !before.contains(f))
        .collect();
    new.sort();
    assert_eq!(new, vec!["show.json".to_string()]);
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    let status = String::from_utf8(out.stdout).unwrap();
    assert!(
        status.lines().all(|l| l.ends_with(" .ig/")),
        "outside .ig/: {status}"
    );
    let text = std::fs::read_to_string(dir.join(ig_app::show::SHOW)).unwrap();
    let show: ig_app::show::Show = serde_json::from_str(&text).unwrap();
    assert_eq!(text, ig_app::show::canonical(&show));
    assert_eq!(show.seq, 1);
    assert_eq!(show.edges, vec![(e.from.clone(), e.to.clone())]);
    call_tool(&dir, "ig_show", args);
    let text = std::fs::read_to_string(dir.join(ig_app::show::SHOW)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["seq"], 2);
}

#[test]
fn unknown_id_is_a_tool_error_and_writes_nothing() {
    let dir = crate::tests::git_fixture("monorepo");
    let r = call_tool(
        &dir,
        "ig_show",
        json!({"title": "t", "nodes": ["packages/core/src/repo.ts#Repo.fnd"]}),
    );
    assert_eq!(r["isError"], true);
    let text = r["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("unknown id packages/core/src/repo.ts#Repo.fnd; did you mean packages/core/src/repo.ts#Repo.find"),
        "{text}"
    );
    assert!(!dir.join(ig_app::show::SHOW).exists());
}

#[test]
fn ig_state_is_the_last_show() {
    let dir = crate::tests::git_fixture("monorepo");
    let r = call_tool(&dir, "ig_state", json!({}));
    assert_eq!(r["structuredContent"], json!({"show": null}));
    let g = head_graph(&dir).unwrap();
    let id = &g.nodes[0].id;
    call_tool(&dir, "ig_show", json!({"title": "t", "nodes": [id]}));
    let r = call_tool(&dir, "ig_state", json!({}));
    assert_eq!(r["structuredContent"]["show"]["seq"], 1);
}

#[test]
fn every_input_schema_is_a_plain_object() {
    for t in schema::tools(Kinds::builtin()).as_array().unwrap() {
        let s = &t["inputSchema"];
        assert_eq!(s["type"], "object", "{}", t["name"]);
        for k in ["oneOf", "anyOf", "allOf"] {
            assert!(s.get(k).is_none(), "{} has a top-level {k}", t["name"]);
        }
    }
}

#[test]
fn ig_show_offers_no_architecture_mode() {
    let t = schema::tools(Kinds::builtin());
    let show = t
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ig_show")
        .unwrap();
    let modes = &show["inputSchema"]["properties"]["mode"]["enum"];
    assert_eq!(modes, &json!(["focus", "chain"]));
    let d = show["description"].as_str().unwrap();
    assert!(d.contains("Draw an answer on the ig map"), "{d}");
}

const SPEC: &str = "system \"shop\" title=\"Shop\" {\n    at \"**\"\n}\n";

fn text(r: &Value) -> String {
    r["content"][0]["text"].as_str().unwrap().to_string()
}

#[test]
fn show_takes_the_map_s_box_ids_and_arrows_where_no_code_is_parsed() {
    let spec = "system \"infra\" title=\"Infra\" {\n    group \"common\" title=\"Common\" {\n        service \"graphify\" title=\"Graphify\" {\n            at \"roles/graphify/**\"\n            writes \"neo4j\"\n        }\n        store \"neo4j\" title=\"Neo4j\" {\n            at \"roles/neo4j/**\"\n        }\n    }\n}\n";
    let dir = crate::tests::tmp_with(&[
        ("ig/system.kdl", spec),
        ("roles/graphify/tasks/main.yml", "- name: x\n"),
        ("roles/neo4j/tasks/main.yml", "- name: y\n"),
    ]);
    let r = call_tool(
        &dir,
        "ig_show",
        json!({"title": "Who writes Neo4j", "nodes": ["graphify", "neo4j"], "edges": [["graphify", "neo4j"]],
            "steps": [{"target": "graphify|neo4j", "text": "writes the graph"}]}),
    );
    assert!(r.get("isError").is_none(), "{r}");
    let r = call_tool(&dir, "ig_show", json!({"title": "t", "nodes": ["grafify"]}));
    assert!(text(&r).contains("did you mean graphify"), "{r}");
    let r = call_tool(
        &dir,
        "ig_show",
        json!({"title": "t", "nodes": ["neo4j"], "edges": [["neo4j", "graphify"]]}),
    );
    assert_eq!(r["isError"], true, "{r}");
}

#[test]
fn plan_without_spec_writes_nothing() {
    let dir = crate::tests::tmp_with(&[("a.txt", "x")]);
    let r = call_tool(&dir, "ig_plan", json!({}));
    assert!(r.get("isError").is_none(), "{r}");
    assert!(text(&r).contains("no ig/ yet"), "{r}");
    assert!(!dir.join("ig").exists() && !dir.join(".ig").exists());
    let r = call_tool(&dir, "ig_edit", json!({"op": "remove", "id": "x"}));
    assert_eq!(r["isError"], true);
    assert!(text(&r).contains("no ig/ yet"), "{r}");
    assert!(!dir.join("ig").exists());
}

#[test]
fn edit_then_plan_records_claude_not_kept() {
    let dir = crate::tests::tmp_with(&[("ig/system.kdl", SPEC), ("web/a.txt", "x")]);
    let add = json!({"op": "add", "parent": "shop", "kind": "app", "id": "web", "title": "Web", "at": ["web/**"]});
    let r = call_tool(&dir, "ig_edit", add);
    assert!(r.get("isError").is_none(), "{r}");
    assert_eq!(r["structuredContent"]["recorded"], json!([1]), "{r}");
    assert!(text(&r).contains("to build: app web"), "{r}");
    let p = call_tool(&dir, "ig_plan", json!({}));
    assert!(text(&p).contains("to build: app web"), "{p}");
    assert!(
        p["structuredContent"]["build"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().starts_with("to build: app web"))
    );
    let j = std::fs::read_to_string(dir.join(".ig/journal.json")).unwrap();
    let j: Value = serde_json::from_str(&j).unwrap();
    assert_eq!(j["entries"][0]["by"], "claude", "{j}");
    assert_eq!(j["entries"][0]["kept"], false);
}

#[test]
fn bad_edit_is_an_error_and_changes_nothing() {
    let dir = crate::tests::tmp_with(&[("ig/system.kdl", SPEC)]);
    let bad = json!({"op": "remove", "id": "nope"});
    let r = call_tool(&dir, "ig_edit", bad);
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(
        std::fs::read_to_string(dir.join("ig/system.kdl")).unwrap(),
        SPEC
    );
    // restore is not Claude's to make
    let r = call_tool(
        &dir,
        "ig_edit",
        json!({"op": "restore", "parent": "shop", "kdl": "x"}),
    );
    assert_eq!(r["isError"], true, "{r}");
}

#[test]
fn batch_stops_at_first_error_and_says_earlier_stay() {
    let dir = crate::tests::tmp_with(&[("ig/system.kdl", SPEC)]);
    let ok = json!({"op": "add", "parent": "shop", "kind": "store", "id": "db", "title": "Db", "at": ["db/**"]});
    let bad = json!({"op": "remove", "id": "nope"});
    let r = call_tool(&dir, "ig_edit", json!({"edits": [ok, bad]}));
    assert_eq!(r["isError"], true, "{r}");
    assert!(text(&r).contains("edit 2"), "{r}");
    assert!(text(&r).contains("journal entry 1 stays recorded"), "{r}");
    assert!(
        std::fs::read_to_string(dir.join("ig/system.kdl"))
            .unwrap()
            .contains("\"db\"")
    );
}

#[test]
fn ig_edit_never_gives_a_box_the_whole_repo() {
    let dir = crate::tests::tmp_with(&[("ig/system.kdl", SPEC)]);
    let add = json!({"op": "add", "parent": "shop", "kind": "app", "id": "all", "title": "All", "at": ["**"]});
    let r = call_tool(&dir, "ig_edit", json!({"edits": [add]}));
    assert_eq!(r["isError"], true, "{r}");
    assert!(text(&r).contains("claims `**`"), "{r}");
    assert_eq!(
        std::fs::read_to_string(dir.join("ig/system.kdl")).unwrap(),
        SPEC
    );
}

#[test]
fn the_tool_schema_lists_the_kinds_of_the_table() {
    let t = schema::tools(Kinds::builtin());
    let edit = t
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ig_edit")
        .unwrap();
    let ops = edit["inputSchema"]["properties"]["edits"]["items"]["oneOf"]
        .as_array()
        .unwrap();
    let kinds = ops[0]["properties"]["kind"]["enum"].as_array().unwrap();
    assert!(
        kinds.contains(&json!("model"))
            && kinds.contains(&json!("agent"))
            && !kinds.contains(&json!("api"))
            && !kinds.contains(&json!("system")),
        "{kinds:?}"
    );
    let arrows = ops
        .iter()
        .find(|o| o["properties"]["op"]["const"] == "arrow")
        .unwrap();
    let arrows = arrows["properties"]["kind"]["enum"].as_array().unwrap();
    assert!(arrows.contains(&json!("writes")) && !arrows.contains(&json!("bundle")));
    let set = ops
        .iter()
        .find(|o| o["properties"]["op"]["const"] == "set")
        .unwrap();
    let key = set["properties"]["key"]["description"].as_str().unwrap();
    assert!(
        key.contains("owner")
            && key.contains("kind renames")
            && key.contains("context")
            && key.contains("prop:<name>"),
        "{key}"
    );
    let arrowset = ops
        .iter()
        .find(|o| o["properties"]["op"]["const"] == "arrowset")
        .unwrap();
    assert_eq!(
        arrowset["properties"]["key"]["enum"],
        json!(["kind", "label", "note", "context", "from", "to", "flip"])
    );
    assert_eq!(arrowset["required"], json!(["op", "from", "to", "key"]));
    assert!(edit["description"].as_str().unwrap().contains("arrowset"));
}

#[test]
fn ig_edit_takes_an_arrowset() {
    let spec = "system \"shop\" title=\"Shop\" {\n    at \"**\"\n    app \"web\" title=\"Web\" {\n        at \"web/**\"\n        uses \"api\"\n    }\n    app \"api\" title=\"Api\" {\n        at \"api/**\"\n    }\n}\n";
    let dir = crate::tests::tmp_with(&[("ig/system.kdl", spec)]);
    let e =
        json!({"op": "arrowset", "from": "web", "to": "api", "key": "label", "value": "orders"});
    let r = call_tool(&dir, "ig_edit", json!({"edits": [e]}));
    assert!(r.get("isError").is_none(), "{r}");
    let kdl = std::fs::read_to_string(dir.join("ig/system.kdl")).unwrap();
    assert!(kdl.contains(r#"uses "api" label="orders""#), "{kdl}");
}

#[test]
fn tools_list_offers_the_teams_kinds_and_ig_edit_adds_one() {
    let dir = crate::tests::tmp_with(&[
        ("ig/system.kdl", SPEC),
        (
            "ig/kinds/iot-device.kdl",
            "kind \"iot-device\" is=\"service\" title=\"IoT device\"\n",
        ),
        ("probe/a.txt", "x"),
    ]);
    let r = handle(
        &dir,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    )
    .unwrap();
    let edit = (r["result"]["tools"].as_array().unwrap().iter())
        .find(|t| t["name"] == "ig_edit")
        .unwrap();
    let kinds = &edit["inputSchema"]["properties"]["edits"]["items"]["oneOf"][0]["properties"]["kind"]
        ["enum"];
    assert!(
        kinds.as_array().unwrap().contains(&json!("iot-device")),
        "{kinds}"
    );
    let add = json!({"op": "add", "parent": "shop", "kind": "iot-device", "id": "probe", "title": "Probe", "at": ["probe/**"]});
    let r = call_tool(&dir, "ig_edit", add);
    assert!(r.get("isError").is_none(), "{r}");
    let sys = std::fs::read_to_string(dir.join("ig/system.kdl")).unwrap();
    assert!(sys.contains("iot-device \"probe\""), "{sys}");
}

#[test]
fn show_draws_box_ids_without_ig() {
    let dir = crate::tests::tmp_with(&[
        ("web/a.ts", "export const a = 1;\n"),
        (
            "web/b.ts",
            "import { a } from './a';\nexport const b = a;\n",
        ),
    ]);
    let w = Wired::new(&dir);
    let v = view::run(&w.env()).unwrap();
    let id = v.boxes[0].id.clone();
    let r = call_tool(&dir, "ig_show", json!({"title": "t", "nodes": [id]}));
    assert!(r.get("isError").is_none(), "{r}");
    assert!(!dir.join("ig").exists());
}

#[test]
fn plan_without_ig_points_to_analyse() {
    let dir = crate::tests::tmp_with(&[("a.txt", "x")]);
    let r = call_tool(&dir, "ig_plan", json!({}));
    assert!(text(&r).contains("ig analyse"), "{r}");
}
