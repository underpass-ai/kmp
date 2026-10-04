use super::{looks_like_option, unknown_option};

/// `kmp-mcp memories [--json]` and `kmp-mcp memories register <absolute-path>`.
///
/// The inventory reads every known store without opening it for use: no
/// lease, no migration, no write. `register` writes only the machine's index
/// of stores, and only for a directory that already is one.
pub(super) fn run(args: &[&str]) -> i32 {
    match args {
        ["register", rest @ ..] => register(rest),
        _ => list(args),
    }
}

fn list(args: &[&str]) -> i32 {
    let mut json = false;
    for argument in args {
        match *argument {
            "--json" => json = true,
            other if looks_like_option(other) => return unknown_option("memories", other),
            other => {
                eprintln!("kmp-mcp memories: unexpected argument `{other}`");
                eprintln!("usage: {}", super::subcommand_usage("memories"));
                return 2;
            }
        }
    }
    let report = if json {
        kmp_mcp::lifecycle::memories_json()
    } else {
        kmp_mcp::lifecycle::memories_report()
    };
    match report {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp memories: {error}");
            1
        }
    }
}

fn register(args: &[&str]) -> i32 {
    let [path] = args else {
        eprintln!("kmp-mcp memories register takes exactly one absolute store path");
        eprintln!("usage: {}", super::subcommand_usage("memories"));
        return 2;
    };
    if looks_like_option(path) {
        return unknown_option("memories", path);
    }
    match kmp_mcp::lifecycle::register_memory(path) {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp memories register: {error}");
            2
        }
    }
}
