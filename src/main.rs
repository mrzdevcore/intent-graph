mod config;
mod embedded;
mod extract;
mod graph;
mod model;
mod patterns;
mod workspace;

fn main() {
    let root = std::env::args().nth(1).unwrap_or(".".into());
    let root = std::path::Path::new(&root);
    let cfg = config::Config::load(root).unwrap();
    let b = graph::build(root, &cfg, None).unwrap();
    println!("{}", serde_json::to_string_pretty(&b.graph).unwrap());
}
