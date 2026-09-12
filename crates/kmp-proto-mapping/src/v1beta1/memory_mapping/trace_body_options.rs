use std::collections::BTreeSet;

use kmp_domain::TraceBodyOptions;
use kmp_proto::v1beta1::TraceSearchOptions;
use tonic::Status;

use super::scalars::invalid_argument;

/// Body delivery options, shared by the target and the seek mode: both
/// materialize the same proof table and both pay for the same bodies.
///
/// A zero ceiling is absent, not "nothing fits": the unbounded legacy read is
/// what no ceiling means, and a caller who wants no body asks for an empty
/// named expansion instead.
pub(super) fn body_options(options: &TraceSearchOptions) -> Result<TraceBodyOptions, Box<Status>> {
    let refs = options
        .proof_refs
        .as_ref()
        .map(|named| {
            let unique: BTreeSet<String> = named.refs.iter().cloned().collect();
            if unique.len() != named.refs.len() {
                return Err(invalid_argument(
                    "search.proof_refs must be distinct; a named expansion delivers each body once",
                ));
            }
            Ok(unique)
        })
        .transpose()?;
    let body = TraceBodyOptions {
        max_record_bytes: (options.max_body_record_bytes > 0)
            .then_some(options.max_body_record_bytes),
        refs,
        expect_selection: (!options.expect_selection.is_empty())
            .then(|| options.expect_selection.clone()),
        compact: (!options.compact_language.is_empty()).then(|| options.compact_language.clone()),
    };
    body.validate()
        .map_err(|error| invalid_argument(error.to_string()))?;
    Ok(body)
}
