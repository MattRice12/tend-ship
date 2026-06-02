fn main() {
    tend_ship::reset_sigpipe();
    std::process::exit(tend_ship::run(std::env::args().collect()));
}
