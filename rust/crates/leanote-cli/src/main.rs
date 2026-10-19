//! leanote CLI — offline minimal surface, byte-compatible with bin/leanote.js.
//! Modes implemented in this ticket: add (incl. stdin), list, show.
//! (search/edit/HTTP mode come with later tickets.)

mod cli;

fn main() {
    let code = cli::run(std::env::args().skip(1).collect());
    std::process::exit(code);
}
