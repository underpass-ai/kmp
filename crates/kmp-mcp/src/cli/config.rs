pub(super) fn run_config_command(args: &[&str]) -> i32 {
    match args {
        [] => match kmp_mcp::agent_policy::load() {
            Ok(policy) => {
                print!("{}", kmp_mcp::agent_policy::display(&policy));
                0
            }
            Err(error) => {
                eprintln!("kmp-mcp: agent policy is invalid: {error}");
                2
            }
        },
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
        ["ask-fallback-languages" | "--ask-fallback-languages", ..] => {
            eprintln!(
                "kmp-mcp: ask-fallback-languages was retired: a semantic question is asked in \
                 English with the user's words as asked_as, and there is no list to configure"
            );
            2
        }
        _ => {
            eprintln!("kmp-mcp: config takes no arguments or `memory-routing <on-request|always>`");
            2
        }
    }
}
