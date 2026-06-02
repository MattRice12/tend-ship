mod dispatch;
mod help;
mod ship;

fn main() {
    std::process::exit(dispatch::run(std::env::args().collect()));
}
