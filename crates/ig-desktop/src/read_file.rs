//! The `read_file` command: one text file of the project, read-only.

use serde::Serialize;
use std::path::{Component, Path};

const MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Serialize, Debug)]
pub struct FileText {
    pub text: String,
    /// From the extension; the viewer picks the editor mode by extension itself.
    pub language: &'static str,
}

fn language(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" => "javascript",
        "css" => "css",
        "html" | "htm" | "vue" => "html",
        "json" => "json",
        _ => "text",
    }
}

/// `path` (relative, no `..`) under `root`; a symlink that leaves the root is refused.
pub fn read_file_for(root: &Path, path: &str) -> Result<FileText, String> {
    let rel = Path::new(path);
    if rel.is_absolute() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(format!("{path}: must be a path inside the project"));
    }
    let abs = root
        .join(rel)
        .canonicalize()
        .map_err(|e| format!("{path}: {e}"))?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if !abs.starts_with(&root) {
        return Err(format!("{path}: outside the project"));
    }
    let meta = std::fs::metadata(&abs).map_err(|e| format!("{path}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{path}: not a regular file"));
    }
    let len = meta.len();
    if len > MAX_BYTES {
        return Err(format!("{path}: too large to show ({len} bytes, max 2 MB)"));
    }
    let bytes = std::fs::read(&abs).map_err(|e| format!("{path}: {e}"))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{path}: not UTF-8 text"))?;
    Ok(FileText {
        text,
        language: language(path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ig-rf-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("proj")).unwrap();
        d.canonicalize().unwrap()
    }

    #[test]
    fn reads_a_file_and_refuses_escapes() {
        let d = tmp("escape");
        let root = d.join("proj");
        std::fs::write(root.join("a.ts"), "let a = 1;\n").unwrap();
        std::fs::write(d.join("x"), "secret").unwrap();
        let f = read_file_for(&root, "a.ts").unwrap();
        assert_eq!(
            (f.text.as_str(), f.language),
            ("let a = 1;\n", "javascript")
        );
        assert!(read_file_for(&root, "../x").is_err());
        assert!(read_file_for(&root, "sub/../a.ts").is_err());
        assert!(read_file_for(&root, d.join("x").to_str().unwrap()).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(d.join("x"), root.join("link")).unwrap();
            let e = read_file_for(&root, "link").unwrap_err();
            assert!(e.contains("outside"), "{e}");
        }
    }

    #[test]
    fn refuses_binary_and_large_files() {
        let d = tmp("size");
        let root = d.join("proj");
        std::fs::write(root.join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(read_file_for(&root, "bin").unwrap_err().contains("UTF-8"));
        std::fs::create_dir(root.join("dir")).unwrap();
        assert!(
            read_file_for(&root, "dir")
                .unwrap_err()
                .contains("regular file")
        );
        std::fs::write(root.join("big"), vec![b'a'; 2 * 1024 * 1024 + 1]).unwrap();
        assert!(
            read_file_for(&root, "big")
                .unwrap_err()
                .contains("too large")
        );
    }
}
