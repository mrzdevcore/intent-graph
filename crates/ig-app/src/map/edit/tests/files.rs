//! Which files an edit writes, in what order, and what a file's last lines become.

use super::*;
use ig_core::config::Config;
use ig_core::ports::PortError;
use ig_core::workspace::WorkspaceInfo;
use std::path::Path;
use std::sync::Mutex;

/// `MemFiles` that logs each write and removal.
struct Logged {
    files: MemFiles,
    log: Mutex<Vec<String>>,
}

impl Files for Logged {
    fn read_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files.read_bytes(path)
    }
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<(), PortError> {
        self.log.lock().unwrap().push(format!("write {path}"));
        self.files.write_atomic(path, bytes)
    }
    fn remove(&self, path: &str) -> Result<(), PortError> {
        self.log.lock().unwrap().push(format!("remove {path}"));
        self.files.remove(path)
    }
    fn exists(&self, path: &str) -> bool {
        self.files.exists(path)
    }
    fn glob(&self, patterns: &[String]) -> Result<Vec<String>, PortError> {
        self.files.glob(patterns)
    }
    fn at(&self, root: &Path) -> Box<dyn Files> {
        self.files.at(root)
    }
    fn discover(&self, config: &Config) -> Result<WorkspaceInfo, PortError> {
        self.files.discover(config)
    }
}

#[test]
fn files_that_gain_nodes_are_written_first_system_kdl_last_then_deletions() {
    let f = Logged {
        files: spec(),
        log: Mutex::default(),
    };
    let take = || std::mem::take(&mut *f.log.lock().unwrap());
    let inv = apply(
        &f,
        &Edit::Move {
            id: "api/orders".into(),
            parent: "web".into(),
            before: None,
            slot: None,
        },
    )
    .unwrap();
    assert_eq!(
        take(),
        [
            "write ig/web.kdl",
            "write ig/api.kdl",
            "write ig/system.kdl"
        ]
    );
    apply(&f, &inv).unwrap();
    assert_eq!(
        take(),
        [
            "write ig/api.kdl",
            "write ig/system.kdl",
            "remove ig/web.kdl"
        ]
    );
}

#[test]
fn a_container_folding_back_is_written_before_its_file_goes() {
    let f = Logged {
        files: MemFiles::with(&[
            ("ig/system.kdl", SYS_WITHOUT_WEB),
            ("ig/api.kdl", API),
            ("ig/web.kdl", WEB),
        ]),
        log: Mutex::default(),
    };
    apply(
        &f,
        &Edit::Remove {
            id: "web/cart".into(),
        },
    )
    .unwrap();
    assert_eq!(
        *f.log.lock().unwrap(),
        ["write ig/system.kdl", "remove ig/web.kdl"]
    );
}

const WEB: &str = r#"// ig's intended architecture: edit freely; ig never rewrites this file.
app "web" title="Web" {
    at "apps/web/**"
    uses "api"
    component "web/cart" title="Cart"
}
// tail
"#;

#[test]
fn a_files_last_comment_moves_with_its_container_and_back() {
    let f = MemFiles::with(&[
        ("ig/system.kdl", SYS_WITHOUT_WEB),
        ("ig/api.kdl", API),
        ("ig/web.kdl", WEB),
    ]);
    check_on(
        f,
        Edit::Remove {
            id: "web/cart".into(),
        },
        &[
            ("ig/web.kdl", ""),
            (
                "ig/system.kdl",
                &SYS_WITHOUT_WEB.replace(
                    "    // end of system\n",
                    r#"    app "web" title="Web" {
        at "apps/web/**"
        uses "api"
    // tail
    }
    // end of system
"#,
                ),
            ),
        ],
    );
}
