//! The base-graph cache key over the real tree and the TS frontend.

use super::{env, tmp_with};
use ig_app::base::cache_key;
use ig_core::config::Config;

#[test]
fn cache_key_tracks_auto_import_contents() {
    let root = tmp_with(&[(".nuxt/imports.d.ts", "export {}\n")]);
    let config = Config {
        auto_imports: vec![".nuxt/*.d.ts".into()],
        ..Config::default()
    };
    let before = cache_key(&env(&root), &config);
    std::fs::write(
        root.join(".nuxt/imports.d.ts"),
        "export { a } from '../a'\n",
    )
    .unwrap();
    assert_ne!(before, cache_key(&env(&root), &config));
}
