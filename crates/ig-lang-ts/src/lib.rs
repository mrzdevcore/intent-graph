//! The TypeScript/JavaScript frontend, single-file components (`embedded`) included: oxc
//! extraction, ast-grep `patterns`, tsconfig-aware resolution, `autoImports`, and checkout
//! preparation. Fs-bound (oxc_resolver reads the tree).

mod autoimports;
mod embedded;
pub mod extract;
mod patterns;
mod prepare;
mod resolve;
mod tsconfig;

use ig_core::config::Config;
use ig_core::ports::{FrontendOutput, LanguageFrontend, PortError};
use ig_core::workspace::{SourceFile, WorkspaceInfo, is_source_ext};
use std::path::Path;

pub struct TsFrontend;

impl LanguageFrontend for TsFrontend {
    fn handles(&self, path: &str, config: &Config) -> bool {
        is_source_ext(path, &config.embedded)
    }

    fn build(
        &self,
        files: &[(SourceFile, String)],
        ws: &WorkspaceInfo,
        config: &Config,
    ) -> FrontendOutput {
        resolve::build(files, ws, config)
    }

    /// node_modules and missing tsconfig `extends` targets always; `autoImports` declaration
    /// files when `link_auto_imports` (base graphs).
    fn prepare_checkout(
        &self,
        work: &WorkspaceInfo,
        checkout_root: &Path,
        config: &Config,
        link_auto_imports: bool,
    ) -> Result<(), PortError> {
        prepare::link_node_modules(work, checkout_root);
        if link_auto_imports {
            prepare::link_missing_auto_imports(&work.root, checkout_root, config);
        }
        Ok(())
    }

    /// The `autoImports` declaration files.
    fn cache_inputs(&self, root: &Path, config: &Config) -> Vec<String> {
        autoimports::files(root, &config.auto_imports).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_frontend_meets_the_contract() {
        ig_core::testing::frontend_contract(
            &TsFrontend,
            "src/a.ts",
            "export function f() { return g(); }\nfunction g() { return 1; }\n",
        );
    }

    #[test]
    fn handles_sources_and_embedded() {
        let config = Config {
            embedded: vec![".vue".into()],
            ..Config::default()
        };
        let h = |p: &str| TsFrontend.handles(p, &config);
        assert!(h("a.ts") && h("b/C.vue") && h("x.mjs"));
        assert!(!h("a.d.ts") && !h("a.css"));
    }
}
