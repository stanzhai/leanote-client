fn main() {
    // winresource embeds icon + manifest into the exe on Windows. The crate
    // is a windows-only build-dependency (see Cargo.toml), so the call is
    // cfg-gated to match; cross-checks from other hosts skip it entirely.
    println!("cargo:rerun-if-changed=../../packaging/leanote.exe.manifest");
    println!("cargo:rerun-if-changed=../../packaging/icon.ico");
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        // concat! only takes literals; env! expands to one, so bake the
        // full path per call instead of building it at runtime.
        let mut res = winresource::WindowsResource::new();
        res.set_icon(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packaging/icon.ico"
        ));
        res.set_manifest_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packaging/leanote.exe.manifest"
        ));
        res.compile().unwrap();
    }
}
