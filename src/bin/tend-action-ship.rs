// tend-action-ship: the same entry point as tend-ship, named the way tend
// discovers extensions (`tend-action-<name>` on PATH). Both binaries are
// installed by `cargo install --path .`; pick whichever name you prefer.

fn main() {
    tend_ship::reset_sigpipe();
    std::process::exit(tend_ship::run(std::env::args().collect()));
}
