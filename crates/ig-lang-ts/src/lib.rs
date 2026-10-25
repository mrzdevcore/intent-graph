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

pub struct TsFrontend {
    embedded: Vec<String>,
}

impl TsFrontend {
    pub fn new(config: &Config) -> TsFrontend {
        TsFrontend {
            embedded: config.embedded.clone(),
        }
    }

    /// The `autoImports` declaration files under `root`, sorted: their contents key the
    /// base-graph cache.
    pub fn declaration_files(root: &Path, config: &Config) -> Vec<String> {
        autoimports::files(root, &config.auto_imports).0
    }
}

impl LanguageFrontend for TsFrontend {
    fn handles(&self, path: &str) -> bool {
        is_source_ext(path, &self.embedded)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_frontend_meets_the_contract() {
        let config = Config::default();
        ig_core::testing::frontend_contract(
            &TsFrontend::new(&config),
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
        let ts = TsFrontend::new(&config);
        assert!(ts.handles("a.ts") && ts.handles("b/C.vue") && ts.handles("x.mjs"));
        assert!(!ts.handles("a.d.ts") && !ts.handles("a.css"));
    }
}
