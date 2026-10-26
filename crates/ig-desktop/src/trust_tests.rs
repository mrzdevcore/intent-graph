//! Tests of the trust gate.

use super::*;

fn scratch(tag: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("ig-trust-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (root, data) = (base.join("p"), base.join("data"));
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    (root, data)
}

fn done(root: &Path) {
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn a_change_to_settings_revokes_trust() {
    let (root, data) = scratch("settings");
    let s = root.join(".claude/settings.json");
    std::fs::write(
        &s,
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"ig hook claude"}]}]}}"#,
    )
    .unwrap();
    let inv = inventory(&root);
    assert_eq!(inv.hooks[0].command, "ig hook claude");
    assert_eq!(inv.hooks[0].when, "after each Claude turn");
    assert!(!is_trusted(&data, &root, &inv));
    trust(&data, &root, &inv).unwrap();
    assert!(is_trusted(&data, &root, &inventory(&root)));
    std::fs::write(
        &s,
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"evil"}]}]}}"#,
    )
    .unwrap();
    assert!(!is_trusted(&data, &root, &inventory(&root)));
    done(&root);
}

#[test]
fn adding_a_file_or_breaking_json_changes_the_hash_without_crashing() {
    let (root, _) = scratch("add");
    let h0 = inventory(&root);
    assert!(!h0.any_files);
    std::fs::write(root.join("AGENTS.md"), "x").unwrap();
    let h1 = inventory(&root);
    assert_ne!(h0.hash, h1.hash);
    assert_eq!(h1.instructions, ["AGENTS.md"]);
    std::fs::write(root.join(".mcp.json"), "{oops").unwrap();
    let h2 = inventory(&root);
    assert_ne!(h1.hash, h2.hash);
    assert_eq!(h2.mcp, ["unreadable .mcp.json"]);
    done(&root);
}

#[test]
fn settings_without_hooks_are_still_described() {
    let (root, _) = scratch("notes");
    std::fs::write(root.join(".claude/settings.json"), r#"{"permissions":{"defaultMode":"bypassPermissions","allow":["Bash(rm:*)"]},"apiKeyHelper":"./k.sh","env":{"A":"1"}}"#).unwrap();
    std::fs::write(
        root.join(".mcp.json"),
        r#"{"mcpServers":{"s":{"command":"npx","args":["-y","x"]}}}"#,
    )
    .unwrap();
    let inv = inventory(&root);
    let all = inv.notes.join("\n");
    for want in [
        "bypassPermissions",
        "Bash(rm:*)",
        "runs: ./k.sh",
        "environment variables: A",
    ] {
        assert!(all.contains(want), "{want} in {all}");
    }
    assert_eq!(inv.mcp, ["s: npx -y x"]);
    assert!(inv.any_files);
    done(&root);
}

#[cfg(unix)]
#[test]
fn symlinked_agent_files_are_followed() {
    let (root, data) = scratch("follow");
    let real = root.join("real");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("c.md"), "a").unwrap();
    std::fs::write(
        real.join("s.json"),
        r#"{"hooks":{"Stop":[{"hooks":[{"command":"ig hook claude"}]}]}}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(real.join("c.md"), root.join("CLAUDE.md")).unwrap();
    std::os::unix::fs::symlink(real.join("s.json"), root.join(".claude/settings.json")).unwrap();
    std::os::unix::fs::symlink(real.join("nope"), root.join("AGENTS.md")).unwrap();
    let inv = inventory(&root);
    assert_eq!(inv.instructions, ["CLAUDE.md"]);
    assert_eq!(inv.hooks[0].command, "ig hook claude");
    assert!(
        inv.notes
            .iter()
            .any(|n| n.starts_with("CLAUDE.md (symlink to"))
    );
    assert_eq!(inv.unreadable, ["AGENTS.md"]);
    trust(&data, &root, &inv).unwrap();
    assert!(is_trusted(&data, &root, &inventory(&root)));
    std::fs::write(real.join("c.md"), "b").unwrap();
    assert!(!is_trusted(&data, &root, &inventory(&root)));
    done(&root);
}

#[test]
fn any_file_under_claude_or_claude_local_md_revokes_trust() {
    let (root, data) = scratch("all");
    let inv = inventory(&root);
    trust(&data, &root, &inv).unwrap();
    std::fs::create_dir_all(root.join(".claude/commands")).unwrap();
    std::fs::write(root.join(".claude/commands/x.md"), "run it").unwrap();
    let inv = inventory(&root);
    assert!(!is_trusted(&data, &root, &inv));
    assert_eq!(inv.instructions, ["command: x"]);
    trust(&data, &root, &inv).unwrap();
    std::fs::write(root.join("CLAUDE.local.md"), "x").unwrap();
    let inv = inventory(&root);
    assert!(!is_trusted(&data, &root, &inv));
    assert!(inv.instructions.contains(&"CLAUDE.local.md".to_string()));
    done(&root);
}

#[test]
fn a_skill_with_allowed_tools_or_hooks_is_called_out_and_big_files_are_skipped() {
    let (root, _) = scratch("skill");
    let sk = root.join(".claude/skills/deploy");
    std::fs::create_dir_all(&sk).unwrap();
    std::fs::write(
        sk.join("SKILL.md"),
        "---\nname: deploy\nallowed-tools:\n  - Bash(git push:*)\n  - Edit\nhooks:\n  Stop: []\n---\nbody",
    )
    .unwrap();
    std::fs::create_dir_all(root.join(".claude/agents")).unwrap();
    std::fs::write(
        root.join(".claude/agents/a.md"),
        "---\nallowed-tools: Read\n---\n",
    )
    .unwrap();
    let big = root.join(".claude/big.bin");
    std::fs::write(&big, vec![0u8; (MAX_FILE + 1) as usize]).unwrap();
    let (a, h1) = (inventory(&root), inventory(&root).hash);
    assert_eq!(a.instructions, ["agent: a", "skill: deploy"]);
    let all = a.notes.join("\n");
    for want in [
        "skill: deploy pre-approves: Bash(git push:*), Edit",
        "skill: deploy runs hooks",
        "agent: a pre-approves: Read",
        "big.bin is too large to inspect",
    ] {
        assert!(all.contains(want), "{want} in {all}");
    }
    std::fs::write(&big, vec![0u8; (MAX_FILE + 2) as usize]).unwrap();
    assert_ne!(h1, inventory(&root).hash);
    done(&root);
}

#[cfg(unix)]
#[test]
fn a_symlink_loop_in_the_plugin_dir_terminates() {
    let (root, _) = scratch("loop");
    let ig = root.join(".claude/skills/ig");
    std::fs::create_dir_all(&ig).unwrap();
    std::os::unix::fs::symlink(&ig, ig.join("self")).unwrap();
    std::fs::write(ig.join("SKILL.md"), "x").unwrap();
    let a = inventory(&root);
    assert!(a.ig_plugin);
    std::fs::write(ig.join("SKILL.md"), "y").unwrap();
    assert_ne!(a.hash, inventory(&root).hash);
    done(&root);
}

#[test]
fn trust_needs_the_hash_and_the_confirm_and_keeps_a_bad_store() {
    let (root, data) = scratch("if");
    let h = inventory(&root).hash;
    assert!(trust_if(&data, &root, "stale", |_| true).is_err());
    assert!(trust_if(&data, &root, &h, |_| false).is_err());
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("trust.json"), "{garbage").unwrap();
    trust_if(&data, &root, &h, |_| true).unwrap();
    assert!(is_trusted(&data, &root, &inventory(&root)));
    assert_eq!(
        std::fs::read(data.join("trust.json.bad")).unwrap(),
        b"{garbage"
    );
    done(&root);
}

#[test]
fn past_the_file_cap_trust_is_not_possible() {
    let (root, data) = scratch("cap");
    let d = root.join(".claude/skills/many");
    std::fs::create_dir_all(&d).unwrap();
    for i in 0..=MAX_FILES {
        std::fs::write(d.join(format!("{i}.md")), "").unwrap();
    }
    let inv = inventory(&root);
    assert_eq!(inv.incomplete.as_deref(), Some(TOO_MANY));
    assert!(trust(&data, &root, &inv).is_err());
    assert_eq!(
        trust_if(&data, &root, &inv.hash, |_| true),
        Err(TOO_MANY.into())
    );
    assert!(!is_trusted(&data, &root, &inv));
    done(&root);
}

#[cfg(unix)]
#[test]
fn an_unreadable_dir_under_claude_cannot_be_trusted() {
    use std::os::unix::fs::PermissionsExt;
    let (root, data) = scratch("unreadable");
    let d = root.join(".claude/agents");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o000)).unwrap();
    let inv = inventory(&root);
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(inv.incomplete.is_some(), "{inv:?}");
    assert!(trust(&data, &root, &inv).is_err());
    assert!(!is_trusted(&data, &root, &inv));
    assert!(inventory(&root).incomplete.is_none(), "readable again");
    done(&root);
}
