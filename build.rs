use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=dashboard/dist");
    let root = env::var("CARGO_MANIFEST_DIR").unwrap();
    let dist = Path::new(&root).join("dashboard/dist");
    assert!(
        dist.join("index.html").is_file(),
        "Build dashboard assets first: npm --prefix dashboard ci && npm --prefix dashboard run build"
    );
    let mut files = Vec::new();
    collect(&dist, &dist, &mut files);
    files.sort();
    let mut generated = String::from("pub const ASSETS: &[(&str, &[u8])] = &[\n");
    for name in files {
        generated.push_str(&format!(
            "({name:?}, include_bytes!({:?})),\n",
            dist.join(&name)
        ));
    }
    generated.push_str("];\n");
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("dashboard_assets.rs"),
        generated,
    )
    .unwrap();
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(
            !kind.is_symlink(),
            "Dashboard build must not contain symlinks"
        );
        if kind.is_dir() {
            collect(root, &entry.path(), files);
        } else if kind.is_file() {
            files.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/"),
            );
        }
    }
}
