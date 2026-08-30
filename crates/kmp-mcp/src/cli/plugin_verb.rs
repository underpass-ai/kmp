use kmp_mcp::lifecycle::NativePluginEngineResolver;
use kmp_mcp::plugin_notice::{NativePluginNotice, PluginNoticeMapper};

pub(super) async fn run_plugin_command(arguments: &[&str]) -> i32 {
    if arguments.first() == Some(&"notice") {
        let owned_arguments = arguments
            .iter()
            .map(|argument| (*argument).to_string())
            .collect::<Vec<_>>();
        let execution = tokio::task::spawn_blocking(move || {
            let arguments = owned_arguments
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            NativePluginNotice::execute(&arguments)
        })
        .await;
        return match execution {
            Ok(Ok(notice)) => {
                if let Some(message) = PluginNoticeMapper::to_text(&notice) {
                    println!("{message}");
                }
                0
            }
            Ok(Err(error)) => {
                eprintln!("kmp-mcp plugin: {error}");
                2
            }
            Err(error) => {
                eprintln!("kmp-mcp plugin notice task failed: {error}");
                2
            }
        };
    }
    match NativePluginEngineResolver::execute(arguments) {
        Ok(resolution) => {
            if let Some(warning) = resolution.warning {
                eprintln!("{warning}");
            }
            println!("KMP_ENGINE={}", resolution.executable);
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp plugin: {error}");
            2
        }
    }
}
