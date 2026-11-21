use std::path::PathBuf;

use super::looks_like_option;

pub(super) async fn run_uninstall_command(args: &[&str]) -> i32 {
    let mut applying = false;
    let mut purge = false;
    let mut keep_memory = false;
    let mut selected_store = None;
    let mut selected_engine = None;
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
            "--engine" => {
                let Some(path) = arguments.next().filter(|path| !looks_like_option(path)) else {
                    eprintln!("kmp-mcp uninstall: --engine requires an absolute path");
                    return 2;
                };
                if selected_engine.replace(PathBuf::from(path)).is_some() {
                    eprintln!("kmp-mcp uninstall: --engine may be given only once");
                    return 2;
                }
            }
            other => {
                eprintln!(
                    "kmp-mcp uninstall: unknown option `{other}`; it takes --store, --engine, \
                     --apply, --purge and --keep-memory"
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
    if selected_store.is_some() && selected_engine.is_some() {
        eprintln!(
            "kmp-mcp uninstall: --store and --engine each select the one thing to remove; give \
             one of them"
        );
        return 2;
    }
    if selected_engine.is_some() && (keep_memory || purge) {
        eprintln!(
            "kmp-mcp uninstall: --engine removes no memory, so --keep-memory and --purge have \
             nothing to say about it"
        );
        return 2;
    }

    let roots = kmp_mcp::lifecycle::native_survey_roots();

    let workspace = roots.working_dir.clone();
    let selective = selected_store.is_some() || selected_engine.is_some();
    let installation = kmp_mcp::lifecycle::NativeInstallationCatalog;
    let store_catalog = kmp_mcp::lifecycle::FilesystemStoreCatalog::new(&roots.data_home);
    let store_index = kmp_mcp::lifecycle::JsonlStoreIndex::new(&roots.data_home);
    let remove = kmp_mcp::lifecycle::RemovePiece::new(&installation);
    let pieces: Vec<_> = if let Some(store) = selected_store.as_deref() {
        match kmp_mcp::lifecycle::SelectStore::new(&installation).execute(store) {
            Ok(piece) => vec![piece],
            Err(reason) => {
                eprintln!("kmp-mcp uninstall: {reason}");
                return 2;
            }
        }
    } else if let Some(engine) = selected_engine.as_deref() {
        match kmp_mcp::lifecycle::SelectEngine::new(
            &installation,
            &kmp_mcp::lifecycle::NativePluginEngineProbe,
        )
        .execute(engine, &roots.home)
        {
            // A selected engine is checked for a live host the same way the
            // survey checks one, so narrowing the command never narrows the
            // protection.
            Ok(piece) => vec![held_if_a_host_is_reading_it(piece, &roots)],
            Err(reason) => {
                eprintln!("kmp-mcp uninstall: {reason}");
                return 2;
            }
        }
    } else {
        kmp_mcp::lifecycle::SurveyInstallation::new(
            &installation,
            &kmp_mcp::lifecycle::NativePluginEngineProbe,
            &store_catalog,
            &store_index,
            &kmp_mcp::lifecycle::NativeProcessLiveness,
        )
        .execute(&roots)
        .into_iter()
        .filter(|piece| !(keep_memory && piece.kind == kmp_mcp::lifecycle::PieceKind::Store))
        .collect()
    };
    print!(
        "{}",
        kmp_mcp::lifecycle::uninstall_report(
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
        .filter(|piece| piece.kind == kmp_mcp::lifecycle::PieceKind::Store)
    {
        match kmp_mcp::lifecycle::StoreRemovalGuard::acquire(&roots.data_home, &piece.path) {
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
        // A copy written inside something this run removes is deleted with
        // it (B3): refuse before anything is written or removed.
        let saves_a_store = pieces
            .iter()
            .any(|piece| piece.kind == kmp_mcp::lifecycle::PieceKind::Store);
        if let Some(piece) = removed_piece_holding(&pieces, &workspace).filter(|_| saves_a_store) {
            println!(
                "kept     {}\n         the saved copy would be written into {}, which this \
                 removes; run the command from another directory",
                piece.path.display(),
                workspace.display()
            );
            println!("\nNothing was removed.");
            return 1;
        }
        for piece in pieces
            .iter()
            .filter(|piece| piece.kind == kmp_mcp::lifecycle::PieceKind::Store)
        {
            match save_store(piece, &workspace).await {
                Ok(Some((destination, events))) => println!(
                    "saved    {} — {events} {}, read back and verified",
                    destination.display(),
                    if events == 1 { "event" } else { "events" }
                ),
                Ok(None) => println!(
                    "empty    {} — 0 events, nothing to save",
                    piece.path.display()
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
        .filter(|piece| piece.kind == kmp_mcp::lifecycle::PieceKind::Store)
    {
        if let Err(reason) = remove.execute(piece) {
            println!("kept     {}\n         {reason}", piece.path.display());
            println!("\nThe store could not be removed; engines and hosts were left in place.");
            return 1;
        }
        println!("removed  {}", piece.path.display());
        let catalog = kmp_mcp::lifecycle::FilesystemStoreCatalog::new(&roots.data_home);
        let index = kmp_mcp::lifecycle::JsonlStoreIndex::new(&roots.data_home);
        if let Err(reason) =
            kmp_mcp::lifecycle::ForgetStore::new(&catalog, &index).execute(&piece.path)
        {
            println!("kept     store index\n         {reason}");
            println!("\nThe store is gone, but its machine-local index could not be updated.");
            return 1;
        }
    }

    let leases = kmp_mcp::lifecycle::store_leases_dir(&roots.data_home);
    let mut kept = 0;
    for piece in pieces
        .iter()
        .filter(|piece| piece.kind != kmp_mcp::lifecycle::PieceKind::Store && piece.path != leases)
    {
        match remove.execute(piece) {
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
        match remove.execute(piece) {
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
        println!(
            "\nThe selected piece is gone; every other KMP store, engine and host was left alone."
        );
    } else {
        println!("\nEverything removable in the list is gone.");
    }
    0
}

/// The same hold the survey would have found, applied to a piece that was
/// selected by path instead of surveyed.
///
/// `--engine` exists so a reader can act on one line of the doctor without
/// reading the rest of the installation. That must not cost them the one
/// check that stops a removal from landing under a running host.
fn held_if_a_host_is_reading_it(
    piece: kmp_mcp::lifecycle::Piece,
    roots: &kmp_mcp::lifecycle::SurveyRoots,
) -> kmp_mcp::lifecycle::Piece {
    let plugin_root = roots.home.join(".claude/plugins/cache/underpass/kmp");
    let Some(version) = piece
        .path
        .ancestors()
        .find(|ancestor| ancestor.parent() == Some(plugin_root.as_path()))
    else {
        return piece;
    };
    let holds = kmp_mcp::lifecycle::SurveyHolds::new(
        &kmp_mcp::lifecycle::NativeInstallationCatalog,
        &kmp_mcp::lifecycle::NativeProcessLiveness,
    );
    kmp_mcp::lifecycle::Piece {
        held_by: holds.execute("claude", version),
        ..piece
    }
}

/// The piece this run would remove that holds `workspace`, if any.
///
/// Paths are compared as the filesystem resolves them, so a store reached
/// through a symlink or a relative spelling is still recognised.
fn removed_piece_holding<'a>(
    pieces: &'a [kmp_mcp::lifecycle::Piece],
    workspace: &std::path::Path,
) -> Option<&'a kmp_mcp::lifecycle::Piece> {
    let resolve =
        |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let workspace = resolve(workspace);
    pieces
        .iter()
        .filter(|piece| piece.refusal().is_none())
        .find(|piece| workspace.starts_with(resolve(&piece.path)))
}

/// Hands one store's memory back as a bundle in `workspace`, before the
/// store goes.
///
/// The copy is published under a name nothing else holds — an earlier
/// uninstall's copy may be the only one left of that memory, so it is never
/// written over (B3) — then read back from disk and verified: the bundle's
/// digest must hold and it must carry exactly the events the store exported.
/// A store with no events has nothing to hand back and leaves no file; an
/// empty copy beside a real one is how the real one used to be lost.
///
/// Returns where the copy is and how many events it holds, or `None` for an
/// empty store.
pub(super) async fn save_store(
    piece: &kmp_mcp::lifecycle::Piece,
    workspace: &std::path::Path,
) -> Result<Option<(PathBuf, u64)>, String> {
    let store = piece.path.as_path();
    let engine = kmp_embedded::default_engine_for_data_dir(store);
    let kernel = kmp_embedded::EmbeddedKernel::open_with_engine(store, engine)
        .map_err(|error| error.to_string())?;
    let bundle = kernel
        .store()
        .export_bundle()
        .await
        .map_err(|error| error.to_string())?;
    let exported = kmp_embedded::verify_bundle(&bundle)
        .map_err(|error| format!("the export does not verify: {error}"))?;
    if exported.event_count == 0 {
        return Ok(None);
    }
    let mut attempt = 1;
    let destination = loop {
        let candidate = piece
            .rescue_candidate(workspace, attempt)
            .ok_or_else(|| "a store always has a rescue path".to_string())?;
        if kmp_embedded::write_bundle_if_absent(&candidate, &bundle)
            .map_err(|error| format!("could not write `{}`: {error}", candidate.display()))?
        {
            break candidate;
        }
        attempt += 1;
    };
    let written = std::fs::read_to_string(&destination)
        .map_err(|error| format!("could not read back `{}`: {error}", destination.display()))?;
    let verified = kmp_embedded::verify_bundle(&written).map_err(|error| {
        format!(
            "the copy at `{}` does not verify: {error}",
            destination.display()
        )
    })?;
    if written != bundle
        || verified.event_count != exported.event_count
        || verified.content_digest != exported.content_digest
    {
        return Err(format!(
            "the copy at `{}` holds {} events, but the store exported {}",
            destination.display(),
            verified.event_count,
            exported.event_count
        ));
    }
    Ok(Some((destination, verified.event_count)))
}
