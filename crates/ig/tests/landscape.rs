use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn folder(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ig-landscape-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for (p, s) in [
        ("web/package.json", r#"{"name":"web"}"#),
        ("web/src/config.ts", "export const u = 'analytics';\n"),
        (
            "web/ig/system.kdl",
            "system \"system\" title=\"Web front\" {\n    at \"**\"\n    service \"front\" title=\"Front\" {\n        at \"src/**\"\n    }\n}\n",
        ),
        ("analytics/go.mod", "module a\n"),
        ("analytics/.env", "SECRET=web\n"),
        ("infra/a.tf", "# deploys web\n"),
        ("web-DM-1/.git", "gitdir: /x/web/.git/worktrees/web-DM-1\n"),
        ("documents/a.txt", "x"),
    ] {
        let f = d.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    }
    d
}

fn files(d: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(d).unwrap().flatten() {
        if e.file_type().unwrap().is_dir() {
            out.extend(files(&e.path()));
        } else {
            out.push(e.path());
        }
    }
    out.sort();
    out
}

fn view(d: &Path) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_ig"))
        .args(["view", "--json", "--no-read", "--root"])
        .arg(d)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn a_folder_of_projects_is_drawn_whole_and_nothing_is_written() {
    let d = folder("whole");
    let before = files(&d);
    let v = view(&d);
    assert_eq!(files(&d), before, "wrote into the folder");
    let boxes = v["boxes"].as_array().unwrap();
    let b = |id: &str| {
        boxes
            .iter()
            .find(|b| b["id"] == id)
            .unwrap_or_else(|| panic!("{id}"))
    };
    assert_eq!(b("web")["title"]["value"], "Web front");
    assert_eq!(b("web/front")["parent"], "web");
    assert_eq!(b("analytics")["needsReading"], true);
    assert_eq!(b("infra")["needsReading"], true);
    let l = &v["landscape"];
    let dirs = |k: &str| {
        l[k].as_array()
            .unwrap()
            .iter()
            .map(|x| x["dir"].as_str().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(dirs("projects"), ["analytics", "infra", "web"]);
    assert_eq!(dirs("leftOut"), ["documents", "web-DM-1"]);
    let links: Vec<_> = v["arrows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["parent"] == "system")
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(links, ["infra->web", "web->analytics"]);
    assert_eq!(v["placed"]["regions"][0]["label"], "Where it runs");
}

#[test]
fn a_folder_with_its_own_map_stays_one_project() {
    let d = folder("own");
    std::fs::create_dir_all(d.join("ig")).unwrap();
    std::fs::write(
        d.join("ig/system.kdl"),
        "system \"system\" title=\"One\" {\n    at \"**\"\n}\n",
    )
    .unwrap();
    let v = view(&d);
    assert!(v.get("landscape").is_none());
    assert_eq!(
        v["boxes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == "system")
            .unwrap()["title"]["value"],
        "One"
    );
}

#[test]
fn an_answer_in_a_folder_names_boxes_by_their_place_in_it() {
    let d = folder("show");
    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ig_show","arguments":{"title":"t","nodes":["web/front","analytics"],"edges":[["web","analytics"]]}}});
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}});
    let mut c = Command::new(env!("CARGO_BIN_EXE_ig"))
        .args(["mcp", "--root"])
        .arg(&d)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    writeln!(c.stdin.as_mut().unwrap(), "{init}\n{call}").unwrap();
    let out = c.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert!(last["result"].get("isError").is_none(), "{last}");
    assert!(d.join(".ig/show.json").is_file());
}
