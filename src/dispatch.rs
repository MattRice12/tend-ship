use std::env;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run(argv: Vec<String>) -> i32 {
    let mut iter = argv.into_iter().skip(1);
    let first = iter.next();
    let rest: Vec<String> = iter.collect();

    match first.as_deref() {
        None => crate::ship::run(&[]),
        Some("ship") => crate::ship::run(&rest),
        Some("help") | Some("--help") | Some("-h") => {
            crate::help::print_help();
            0
        }
        Some("--version") | Some("-V") => {
            println!("tend-ship {VERSION}");
            0
        }
        Some(name) => exec_extension(name, &rest),
    }
}

fn exec_extension(name: &str, args: &[String]) -> i32 {
    let bin = format!("tend-ship-{name}");
    match which(&bin) {
        Some(path) => {
            let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let claude_home = dirs_home().join(".claude");
            let project_dir = encoded_project_dir(&cwd, &claude_home);

            let err = Command::new(&path)
                .args(args)
                .env("TEND_SHIP_VERSION", VERSION)
                .env("TEND_SHIP_CWD", &cwd)
                .env("TEND_SHIP_HOME", &claude_home)
                .env("TEND_SHIP_PROJECT", project_dir.unwrap_or_default())
                .exec();

            eprintln!("tend-ship: failed to exec {}: {err}", path.display());
            127
        }
        None => {
            eprintln!(
                "tend-ship: '{name}' is not a tend-ship subcommand. \
                 See 'tend-ship help'."
            );
            2
        }
    }
}

pub fn which(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(name);
            if candidate.is_file() && is_executable(&candidate) {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn dirs_home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn encoded_project_dir(cwd: &Path, claude_home: &Path) -> Option<PathBuf> {
    let encoded = crate::encode::encode_path(cwd)?;
    let candidate = claude_home.join("projects").join(encoded);
    candidate.is_dir().then_some(candidate)
}
