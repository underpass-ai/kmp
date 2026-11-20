//! What to tell a person whose live store and committed bundle disagree.
//!
//! One wording for the write guard, `export`, `doctor` and the read notice,
//! so a refusal and a diagnosis never send someone two different ways. Each
//! names the command that resolves its state.

use std::path::Path;

use kmp_adapter_embedded::HistoryRelation;

/// The flag `export` takes to replace a committed bundle that is not a
/// prefix of the store.
pub const OVERWRITE_COMMITTED_FLAG: &str = "--overwrite-committed";

/// The two histories are branches. Both ways out are named; neither is
/// taken for the person.
pub fn diverged_advice(bundle: &Path, data_dir: &Path, relation: &HistoryRelation) -> String {
    let position = match relation {
        HistoryRelation::Diverged { position, .. } => *position,
        _ => 0,
    };
    format!(
        "the committed memory `{bundle}` ({committed} events) and the live store `{store}` \
         ({live} events) are different histories: they first differ at event {position}, and \
         KMP will not merge two branches. To keep the committed history, move `{store}` aside \
         (for example `mv {store} {store}.mine`; it stays gitignored) and reopen: the next \
         session rebuilds the store from the bundle. An about only this store wrote comes back \
         with `kmp-mcp import --from {store}.mine --about <about>` and then `kmp-mcp export`; \
         what this store added to an about the bundle also changed cannot be merged and has to \
         be written again. To keep this store's history instead and drop the events only the \
         bundle holds, run `kmp-mcp export {flag}` and commit the result.",
        bundle = bundle.display(),
        store = data_dir.display(),
        committed = relation.bundle_events(),
        live = relation.store_events(),
        flag = OVERWRITE_COMMITTED_FLAG,
    )
}

/// The store holds writes the bundle never checkpointed and nothing marks
/// them as an interrupted write, so they are not published on their own.
pub fn store_ahead_advice(bundle: &Path, relation: &HistoryRelation) -> String {
    format!(
        "the committed memory `{bundle}` is behind the live store ({committed} committed events, \
         {live} live): run `kmp-mcp export` to checkpoint them — it keeps every committed event \
         — then commit the bundle",
        bundle = bundle.display(),
        committed = relation.bundle_events(),
        live = relation.store_events(),
    )
}
