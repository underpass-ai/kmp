use super::memory_store_config;

pub(super) fn run_config_command(args: &[&str]) -> i32 {
    match args {
        [] => show_everything(),
        ["memory-routing" | "--memory-routing", value] => {
            let routing = match kmp_mcp::agent_policy::MemoryRouting::parse(value) {
                Ok(routing) => routing,
                Err(error) => {
                    eprintln!("kmp-mcp: {error}");
                    return 2;
                }
            };
            match kmp_mcp::agent_policy::store_memory_routing(routing) {
                Ok(policy) => {
                    print!("{}", kmp_mcp::agent_policy::display(&policy));
                    0
                }
                Err(error) => {
                    eprintln!("kmp-mcp: could not store agent policy: {error}");
                    2
                }
            }
        }
        ["memory-store" | "--memory-store", "--clear"] => report(memory_store_config::clear()),
        ["memory-store" | "--memory-store", path] => report(memory_store_config::save(path)),
        ["ask-fallback-languages" | "--ask-fallback-languages", ..] => {
            eprintln!(
                "kmp-mcp: ask-fallback-languages was retired: a semantic question is asked in \
                 English with the user's words as asked_as, and there is no list to configure"
            );
            2
        }
        _ => {
            eprintln!(
                "kmp-mcp: config takes no arguments, `memory-routing <on-request|always>`, or \
                 `memory-store <absolute-path>|--clear`"
            );
            2
        }
    }
}

/// Both halves of the user configuration: how an agent enters memory, and
/// which memory it enters. They live in one file and are read as one answer.
fn show_everything() -> i32 {
    let policy = match kmp_mcp::agent_policy::load() {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("kmp-mcp: agent policy is invalid: {error}");
            return 2;
        }
    };
    print!("{}", kmp_mcp::agent_policy::display(&policy));
    println!();
    report(memory_store_config::describe())
}

fn report(outcome: Result<String, String>) -> i32 {
    match outcome {
        Ok(rendered) => {
            print!("{rendered}");
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}
