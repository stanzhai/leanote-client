fn main() {
    // winresource embeds icon + manifest into the exe on Windows. The crate
    // is a windows-only build-dependency (see Cargo.toml), so the call is
    // cfg-gated to match; cross-checks from other hosts skip it entirely.
    println!("cargo:rerun-if-changed=../../packaging/leanote.exe.manifest");
    println!("cargo:rerun-if-changed=../../packaging/icon.ico");
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        let manifest = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packaging/leanote.exe.manifest"
        );
        winresource::compile(manifest).unwrap();
    }
}
