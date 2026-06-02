use std::env;
use std::os::unix::fs::PermissionsExt;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn print_help() {
    println!("tend-ship {VERSION}");
    println!();
    println!("Commit and push using the current Claude Code session's transcript.");
    println!();
    println!("USAGE:");
    println!("    tend-ship [SUBCOMMAND] [ARGS...]");
    println!();
    println!("BUILT-IN SUBCOMMANDS:");
    println!("    ship      Commit and push using the current session (default)");
    println!("    help      Show this help");
    println!();
    println!("EXTENSIONS (PATH-discovered):");
    let exts = discover_extensions();
    if exts.is_empty() {
        println!(
            "    (none installed; create an executable named `tend-ship-<name>` in PATH)"
        );
    } else {
        for name in exts {
            println!("    {name}");
        }
    }
    println!();
    println!("OPTIONS:");
    println!("    -h, --help        Show this help");
    println!("    -V, --version     Show version");
    println!();
    println!("Run `tend-ship ship --help` for ship-subcommand options.");
}

fn discover_extensions() -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let Some(paths) = env::var_os("PATH") else {
        return found;
    };
    for dir in env::split_paths(&paths) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            let Some(suffix) = name_str.strip_prefix("tend-ship-") else {
                continue;
            };
            if is_executable(&entry.path()) {
                found.push(suffix.to_string());
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

fn is_executable(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
