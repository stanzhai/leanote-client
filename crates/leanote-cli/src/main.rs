//! leanote CLI — full surface, byte-compatible with bin/leanote.js:
//! add/list/show/search/edit (all stdin-aware), dual-mode HTTP probe,
//! first-run shim switch.

mod cli;

fn main() {
    let code = cli::run(std::env::args().skip(1).collect());
    std::process::exit(code);
}
