fn main() {
    // winresource embeds icon + manifest into the exe on Windows. The crate
    // is a windows-only build-dependency (see Cargo.toml), so the call is
    // cfg-gated to match; cross-checks from other hosts skip it entirely.
    println!("cargo:rerun-if-changed=../../packaging/leanote.exe.manifest");
    println!("cargo:rerun-if-changed=../../packaging/icon.ico");
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging");
        // Icon comes from [package.metadata.winresource]; the manifest file is
        // set explicitly. `compile()` is a method on WindowsResource — the
        // old free-function call never compiled.
        let mut res = winresource::WindowsResource::new();
        res.set_icon(concat!(root, "/icon.ico"));
        res.set_manifest_file(concat!(root, "/leanote.exe.manifest"));
        res.compile().unwrap();
    }
}
