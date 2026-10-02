//! `Cargo.toml` denies `unsafe_code` for every target. This proof keeps the
//! only exemption on `src/terminal/attributes.rs`, so `unsafe` code anywhere
//! else fails to compile.

use std::{fs, path::Path};

const EXEMPTION: &str = "#[allow(unsafe_code)]\nmod attributes;\n";

#[test]
fn unsafe_code_is_denied_everywhere_but_the_terminal_attributes_module() {
    let manifest = fs::read_to_string("Cargo.toml").unwrap();
    assert!(
        manifest
            .contains("[lints.rust]\nunsafe_code = \"deny\"\nunsafe_op_in_unsafe_fn = \"deny\"\n")
    );
    assert!(manifest.contains("[lints.clippy]\nundocumented_unsafe_blocks = \"deny\"\n"));
    let mut exemptions = Vec::new();
    for root in ["src", "tests"] {
        collect(Path::new(root), &mut exemptions);
    }
    assert_eq!(exemptions, ["src/terminal.rs"]);
    let terminal = fs::read_to_string("src/terminal.rs").unwrap();
    assert_eq!(terminal.matches("unsafe_code").count(), 1);
    assert!(terminal.contains(EXEMPTION));
}

/// Pushes every Rust file under `directory` that mentions a lint level for
/// `unsafe_code`, other than this proof.
fn collect(directory: &Path, found: &mut Vec<String>) {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, found);
        } else if mentions_the_lint(&path) {
            found.push(path.to_string_lossy().into_owned());
        }
    }
}

fn mentions_the_lint(path: &Path) -> bool {
    let rust = path.extension().is_some_and(|extension| extension == "rs");
    rust && path != Path::new("tests/unsafe_boundary.rs")
        && fs::read_to_string(path).unwrap().contains("unsafe_code")
}
