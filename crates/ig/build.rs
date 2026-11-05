//! Embeds the viewer: `viewer/dist/index.html` (built by `make viewer`) becomes `OUT_DIR/view.html`.
//! Without it, a placeholder page that shows the data as JSON takes its place, so Rust-only work
//! still builds without pnpm or node.

use std::path::Path;

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ig — system map</title>
</head>
<body>
<p>viewer not built: run <code>make viewer</code></p>
<pre id="raw"></pre>
<script id="ig-data" type="application/json">/*IG_DATA*/</script><script id="ig-show" type="application/json">/*IG_SHOW*/</script>
<script id="ig-live" type="application/json">/*IG_LIVE*/</script>
<script>
var raw = document.getElementById("ig-data").textContent;
try { raw = JSON.stringify(JSON.parse(raw), null, 2); } catch (e) {}
document.getElementById("raw").textContent = raw;
</script>
</body>
</html>
"#;

fn main() {
    let dist = concat!(env!("CARGO_MANIFEST_DIR"), "/../../viewer/dist/index.html");
    println!("cargo:rerun-if-changed={dist}");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).to_path_buf();
    std::fs::write(out.join("placeholder.html"), PLACEHOLDER).unwrap();
    let page = std::fs::read_to_string(dist).unwrap_or_else(|_| {
        println!("cargo:warning=viewer/dist/index.html missing; run make viewer");
        PLACEHOLDER.to_string()
    });
    std::fs::write(out.join("view.html"), page).unwrap();
}
