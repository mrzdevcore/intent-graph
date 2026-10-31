//! `ig fmt` on files in memory: indentation and blank lines change, comments do not.

use super::*;
use crate::map::edit::fmt::changed_comment;

/// Comments in every place a person writes them, the indentation mangled. (kdl cannot read
/// a comment after a `}` with siblings after it.)
const MANGLED: &str = r#"

// ig's intended architecture: edit freely; ig never rewrites this file.

// the shop
  system "shop" title="Shop" { // the root
        at "**" // everything
  store "db" /* inline */ title="Postgres"
        external "stripe" title="Stripe"
       /* a block
          over lines */



      app "web" title="Web" {
  at "apps/web/**"
          uses "db" // reads it
   } // after the web block
}
// eof
"#;

const FORMATTED: &str = r#"// ig's intended architecture: edit freely; ig never rewrites this file.
// the shop
system "shop" title="Shop" { // the root
    at "**" // everything
    store "db" /* inline */ title="Postgres"
    external "stripe" title="Stripe"
    /* a block
          over lines */

    app "web" title="Web" {
        at "apps/web/**"
        uses "db" // reads it
    } // after the web block
}
// eof
"#;

#[test]
fn fmt_reindents_and_keeps_every_comment() {
    let f = MemFiles::with(&[("ig/system.kdl", MANGLED)]);
    assert_eq!(fmt(&f, false).unwrap(), vec!["ig/system.kdl".to_string()]);
    assert_eq!(texts(&f)["ig/system.kdl"], MANGLED, "check writes nothing");
    assert_eq!(fmt(&f, true).unwrap(), vec!["ig/system.kdl".to_string()]);
    assert_eq!(texts(&f)["ig/system.kdl"], FORMATTED);
    assert!(
        fmt(&f, false).unwrap().is_empty(),
        "formatted stays formatted"
    );
}

#[test]
fn fmt_changes_only_the_old_names_in_the_fixture() {
    let f = spec();
    assert_eq!(fmt(&f, true).unwrap(), vec!["ig/api.kdl".to_string()]);
    let want = API
        .replace("data \"db\"", "reads \"db\"")
        .replace("network", "calls");
    assert_eq!(texts(&f)["ig/api.kdl"], want);
}

#[test]
fn fmt_writes_the_new_names_and_keeps_comments() {
    let old = "system \"shop\" title=\"Shop\" {\n    at \"**\"\n    api \"a\" title=\"A\" { // why\n        depends \"b\"\n    }\n    worker \"b\" title=\"B\"\n    folder \"f\" title=\"F\" {\n        library \"l\" title=\"L\"\n    }\n}\n";
    let f = MemFiles::with(&[("ig/system.kdl", old)]);
    fmt(&f, true).unwrap();
    let want = old
        .replace("api \"a\"", "service \"a\"")
        .replace("depends", "uses")
        .replace("worker", "service")
        .replace("folder", "group")
        .replace("library", "module");
    assert_eq!(texts(&f)["ig/system.kdl"], want);
}

#[test]
fn a_changed_comment_is_found_with_its_line() {
    assert_eq!(changed_comment("a // x\n", "a // x\n"), None);
    assert_eq!(changed_comment("a\nb // x\n", "a\nb\n"), Some(2));
    assert_eq!(changed_comment("a\n", "a\n/* y */\n"), Some(2));
    // a comment marker inside a string is not a comment
    assert_eq!(changed_comment("a \"// no\"\n", "a\n"), None);
}
