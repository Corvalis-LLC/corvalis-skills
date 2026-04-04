use std::path::PathBuf;

#[cfg(target_os = "macos")]
use std::env;
#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::os::unix::fs::PermissionsExt;

fn compile_grammar(name: &str, dir: &str) {
    let vendor_root = PathBuf::from("vendor");
    let vendor = vendor_root.join(dir);

    let mut build = cc::Build::new();
    configure_archiver(&mut build);
    build
        .include(&vendor)
        .include(&vendor_root)
        .file(vendor.join("parser.c"))
        .warnings(false);

    let scanner = vendor.join("scanner.c");
    if scanner.exists() {
        build.file(scanner);
    }

    build.compile(name);
}

#[cfg(target_os = "macos")]
fn configure_archiver(build: &mut cc::Build) {
    if let Ok(wrapper) = write_macos_ar_wrapper() {
        build.archiver(wrapper);
    }
}

#[cfg(not(target_os = "macos"))]
fn configure_archiver(_build: &mut cc::Build) {}

#[cfg(target_os = "macos")]
fn write_macos_ar_wrapper() -> std::io::Result<PathBuf> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR should be set"));
    let wrapper = out_dir.join("apple-ar-wrapper.sh");
    let script = r#"#!/bin/sh
set -eu

if [ "$#" -eq 0 ]; then
  exec /usr/bin/ar
fi

mode="$(printf '%s' "$1" | tr -d 'D')"
if [ -z "$mode" ]; then
  mode="$1"
fi
shift

exec /usr/bin/ar "$mode" "$@"
"#;

    fs::write(&wrapper, script)?;
    let mut permissions = fs::metadata(&wrapper)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&wrapper, permissions)?;
    Ok(wrapper)
}

fn main() {
    compile_grammar("tree-sitter-javascript", "javascript");
    compile_grammar("tree-sitter-typescript", "typescript");
    compile_grammar("tree-sitter-tsx", "tsx");
    compile_grammar("tree-sitter-svelte", "svelte");

    println!("cargo:rerun-if-changed=vendor/");
    println!("cargo:rerun-if-changed=common/scanner.h");
}
