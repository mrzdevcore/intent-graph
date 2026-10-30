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
fn fmt_leaves_the_fixture_as_it_is() {
    let f = spec();
    assert!(fmt(&f, false).unwrap().is_empty());
}

#[test]
fn a_changed_comment_is_found_with_its_line() {
    assert_eq!(changed_comment("a // x\n", "a // x\n"), None);
    assert_eq!(changed_comment("a\nb // x\n", "a\nb\n"), Some(2));
    assert_eq!(changed_comment("a\n", "a\n/* y */\n"), Some(2));
    // a comment marker inside a string is not a comment
    assert_eq!(changed_comment("a \"// no\"\n", "a\n"), None);
}
