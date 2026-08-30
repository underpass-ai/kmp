//! `kmp-mcp uninstall` — removal that protects memory before it removes anything.
//!
//! One concept: the operator's removal path. It previews by default, rescues
//! the memory it is about to orphan, and only then applies — so the rescue and
//! the removal are read together, here, rather than one being found without
//! the other.

use super::help::looks_like_option;
use std::path::PathBuf;

/// `uninstall` — what `/kmp:setup` never had an inverse for.
///
/// The dry run is the default and `--apply` is how someone says to go ahead.
/// `--store <absolute-path>` narrows both preview and apply to exactly one
/// memory. Every selected store must be inactive before any part of the plan
/// is removed, so uninstall never fixes contention by killing a host.
pub(super) async fn run(args: &[&str]) -> i32 {
    let mut applying = false;
    let mut purge = false;
    let mut keep_memory = false;
    let mut selected_store = None;
    let mut arguments = args.iter();
    while let Some(argument) = arguments.next() {
        match *argument {
            "--apply" | "--yes" => applying = true,
            "--purge" => purge = true,
            "--keep-memory" => keep_memory = true,
            "--store" => {
                let Some(path) = arguments.next().filter(|path| !looks_like_option(path)) else {
                    eprintln!("kmp-mcp uninstall: --store requires an absolute path");
                    return 2;
                };
                if selected_store.replace(PathBuf::from(path)).is_some() {
                    eprintln!("kmp-mcp uninstall: --store may be given only once");
                    return 2;
                }
            }
            other => {
                eprintln!(
                    "kmp-mcp uninstall: unknown option `{other}`; it takes --store, --apply, \
                     --purge and --keep-memory"
                );
                return 2;
            }
        }
    }
    if selected_store.is_some() && keep_memory {
        eprintln!(
            "kmp-mcp uninstall: --store and --keep-memory conflict; the selected store is the \
             only thing this command would remove"
        );
        return 2;
    }

    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let data_home =
        kmp_embedded::user_data_home().unwrap_or_else(|| home.join(".local").join("share"));
    let roots = kmp_mcp::uninstall::Roots {
        home,
        data_home,
        working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        path_entries: std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default(),
    };

    let workspace = roots.working_dir.clone();
    let selective = selected_store.is_some();
    let pieces: Vec<_> = if let Some(store) = selected_store.as_deref() {
        match kmp_mcp::uninstall::selected_store(store) {
            Ok(piece) => vec![piece],
            Err(reason) => {
                eprintln!("kmp-mcp uninstall: {reason}");
                return 2;
            }
        }
    } else {
        kmp_mcp::uninstall::survey(&roots)
            .into_iter()
            .filter(|piece| !(keep_memory && piece.kind == kmp_mcp::uninstall::PieceKind::Store))
            .collect()
    };
    print!(
        "{}",
        kmp_mcp::uninstall::report(
            &pieces,
            &workspace,
            purge,
            applying,
            kmp_mcp::style::Style::for_stdout()
        )
    );

    if pieces.is_empty() {
        return 0;
    }
    if !applying {
        println!("Run the same command with --apply to remove what is listed.");
        return 0;
    }

    // This preflight comes before export, engine removal, plugin removal or
    // any other mutation. A live owner is a reason to stop, never a process
    // uninstall is authorised to kill.
    let mut store_guards = Vec::new();
    for piece in pieces
        .iter()
        .filter(|piece| piece.kind == kmp_mcp::uninstall::PieceKind::Store)
    {
        match kmp_mcp::uninstall::StoreRemovalGuard::acquire(&roots.data_home, &piece.path) {
            Ok(guard) => store_guards.push(guard),
            Err(reason) => {
                println!("kept     {}\n         {reason}", piece.path.display());
                println!("\nNothing was removed.");
                return 1;
            }
        }
    }

    // Memory is handed back before anything is taken. Export every selected
    // store first; a later export failure therefore leaves the installation
    // intact instead of producing a half-uninstall.
    if !purge {
        for piece in pieces
            .iter()
            .filter(|piece| piece.kind == kmp_mcp::uninstall::PieceKind::Store)
        {
            let destination = kmp_mcp::uninstall::rescue_path(piece, &workspace)
                .expect("a store always has a rescue path");
            match save_store(&piece.path, &destination).await {
                Ok(events) => println!(
                    "saved    {} — {events} {}",
                    destination.display(),
                    if events == 1 { "event" } else { "events" }
                ),
                Err(reason) => {
                    println!(
                        "kept     {}\n         could not save it first: {reason}",
                        piece.path.display()
                    );
                    println!("\nNothing was removed.");
                    return 1;
                }
            }
        }
    }

    // Stores go first. If their bytes or their exact index entries cannot be
    // retired, preserve every engine and host integration for a clean retry.
    for piece in pieces
        .iter()
        .filter(|piece| piece.kind == kmp_mcp::uninstall::PieceKind::Store)
    {
        if let Err(reason) = kmp_mcp::uninstall::remove(piece) {
            println!("kept     {}\n         {reason}", piece.path.display());
            println!("\nThe store could not be removed; engines and hosts were left in place.");
            return 1;
        }
        println!("removed  {}", piece.path.display());
        if let Err(reason) = kmp_mcp::memories::forget(&roots.data_home, &piece.path) {
            println!("kept     store index\n         {reason}");
            println!("\nThe store is gone, but its machine-local index could not be updated.");
            return 1;
        }
    }

    let leases = kmp_mcp::uninstall::store_leases_dir(&roots.data_home);
    let mut kept = 0;
    for piece in pieces
        .iter()
        .filter(|piece| piece.kind != kmp_mcp::uninstall::PieceKind::Store && piece.path != leases)
    {
        match kmp_mcp::uninstall::remove(piece) {
            Ok(()) => println!("removed  {}", piece.path.display()),
            Err(reason) => {
                kept += 1;
                println!("kept     {}\n         {reason}", piece.path.display());
            }
        }
    }

    // A full uninstall may remove the coordination directory only after all
    // exclusive handles have been released. Selective removal deliberately
    // keeps it so future hosts and retries coordinate on the same identity.
    drop(store_guards);
    for piece in pieces.iter().filter(|piece| piece.path == leases) {
        match kmp_mcp::uninstall::remove(piece) {
            Ok(()) => println!("removed  {}", piece.path.display()),
            Err(reason) => {
                kept += 1;
                println!("kept     {}\n         {reason}", piece.path.display());
            }
        }
    }
    if kept > 0 {
        println!("\n{kept} left in place. KMP is not fully removed.");
        return 1;
    }
    if selective {
        println!("\nThe selected store is gone; every other KMP store and host was left alone.");
    } else {
        println!("\nEverything removable in the list is gone.");
    }
    0
}

/// Writes a store's whole event log to `destination`, and answers with how
/// many events it holds — a number is what makes the file believable.
async fn save_store(store: &std::path::Path, destination: &std::path::Path) -> Result<u64, String> {
    let engine = kmp_embedded::default_engine_for_data_dir(store);
    let kernel = kmp_embedded::EmbeddedKernel::open_with_engine(store, engine)
        .map_err(|error| error.to_string())?;
    let bundle = kernel
        .store()
        .export_bundle()
        .await
        .map_err(|error| error.to_string())?;
    let events = bundle
        .lines()
        .next()
        .and_then(|header| serde_json::from_str::<serde_json::Value>(header).ok())
        .and_then(|header| {
            header
                .get("event_count")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or_default();
    std::fs::write(destination, bundle)
        .map_err(|error| format!("could not write `{}`: {error}", destination.display()))?;
    Ok(events)
}
