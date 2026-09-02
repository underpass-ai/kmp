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
        ["ask-fallback-languages" | "--ask-fallback-languages", value] => {
            let languages = match kmp_mcp::agent_policy::parse_cli_languages(value) {
                Ok(languages) => languages,
                Err(error) => {
                    eprintln!("kmp-mcp: {error}");
                    return 2;
                }
            };
            match kmp_mcp::agent_policy::store(&languages) {
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
        _ => {
            eprintln!(
                "kmp-mcp: config takes no arguments, `memory-routing <on-request|always>`, or `ask-fallback-languages <comma-separated-tags|none>`"
            );
            2
        }
    }
}
