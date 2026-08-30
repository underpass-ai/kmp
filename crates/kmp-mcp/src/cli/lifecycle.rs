//! `kmp-mcp setup|update` — the install and converge verbs, over the lifecycle context.

use kmp_mcp::lifecycle::{LifecycleAction, LifecycleFailureMapper, NativeLifecycle};

pub(super) async fn run(action: LifecycleAction, arguments: &[&str]) -> i32 {
    let owned_arguments = arguments
        .iter()
        .map(|argument| (*argument).to_string())
        .collect::<Vec<_>>();
    let execution = tokio::task::spawn_blocking(move || {
        let arguments = owned_arguments
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        NativeLifecycle::execute(action, &arguments)
    })
    .await;
    let output = match execution {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            let failure = LifecycleFailureMapper::to_dto(action, &error);
            match serde_json::to_string_pretty(&failure) {
                Ok(json) => println!("{json}"),
                Err(serialize_error) => {
                    eprintln!("kmp-mcp: could not serialize lifecycle failure: {serialize_error}")
                }
            }
            return 1;
        }
        Err(error) => {
            eprintln!("kmp-mcp: lifecycle worker failed: {error}");
            return 1;
        }
    };
    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: could not serialize lifecycle receipt: {error}");
            2
        }
    }
}
