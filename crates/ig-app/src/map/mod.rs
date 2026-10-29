//! The system map: the boxes ig sees (system, containers, components, code) and the arrows
//! between them, lifted from the graph; the map file; its layers; and the view `ig view` draws.

pub mod agent;
pub mod ansible;
pub mod arrows;
pub mod code;
pub mod externals;
pub mod facts;
pub mod file;
pub mod layers;
pub mod layout;
pub mod route;
pub mod view;

pub use arrows::{ArrowKind, MapArrow, arrows};
pub use facts::{BoxKind, FactBox, Guess, Level, Pattern, facts};
pub use layers::{Mark, MarkState};
pub use view::{View, view};

#[cfg(test)]
pub(crate) mod shop {
    //! The `shop` fixture as the real pipeline sees it: its golden graph (kept in sync by the
    //! `ig` golden test) and its workspace.
    use ig_core::config::Config;
    use ig_core::model::Graph;
    use ig_core::workspace::{Package, SourceFile, WorkspaceInfo};
    use std::path::PathBuf;

    pub fn load() -> (WorkspaceInfo, Graph, Config) {
        let mut g: serde_json::Value =
            serde_json::from_str(include_str!("../../../ig/tests/golden/shop.graph.json")).unwrap();
        g["toolVersion"] = "test".into(); // the golden blanks it
        let g: Graph = serde_json::from_value(g).unwrap();
        let cfg = Config::parse(include_str!("../../../ig/tests/fixtures/shop/ig.yaml")).unwrap();
        let packages: Vec<Package> = [
            ("@shop/api", "apps/api"),
            ("@shop/web", "apps/web"),
            ("@shop/api-client", "packages/api-client"),
        ]
        .iter()
        .map(|(name, dir)| Package {
            name: name.to_string(),
            dir: dir.to_string(),
            tsconfig: None,
            source_entry: None,
        })
        .collect();
        let files = g
            .nodes
            .iter()
            .filter(|n| n.is_module())
            .map(|n| SourceFile {
                path: n.file.clone(),
                package: packages
                    .iter()
                    .position(|p| n.file.starts_with(&format!("{}/", p.dir)))
                    .unwrap(),
            })
            .collect();
        let ws = WorkspaceInfo {
            root: PathBuf::from("shop"),
            packages,
            files,
        };
        (ws, g, cfg)
    }
}
