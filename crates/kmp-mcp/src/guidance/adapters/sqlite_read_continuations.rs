use rusqlite::{OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::random_agent_identity::RandomAgentIdentity;
use super::sqlite_agent_directory::{SqliteAgentDirectory, storage};
use crate::guidance::{AgentSession, GuidanceError, ReadContinuation, ReadContinuationId};

const MAX_CALL_BYTES: usize = 32 * 1024;

#[cfg(test)]
#[path = "sqlite_read_continuations_tests.rs"]
mod tests;

pub(super) fn save(
    directory: &SqliteAgentDirectory,
    session: &AgentSession,
    call: &ReadContinuation,
) -> Result<ReadContinuationId, GuidanceError> {
    let arguments = serde_json::to_string(&call.arguments).map_err(storage)?;
    if arguments.len() > MAX_CALL_BYTES {
        return Err(storage("continuation exceeds the retained-call allowance"));
    }
    if call.arguments["context_id"] != session.context_id.as_str() {
        return Err(GuidanceError::InvalidSession(
            "continuation context must remain bound".into(),
        ));
    }
    let mut digest = Sha256::new();
    digest.update(call.tool.as_bytes());
    digest.update([0]);
    digest.update(arguments.as_bytes());
    let fingerprint = format!("{:x}", digest.finalize());
    let mut connection = directory.connection.lock().map_err(storage)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let known: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM contexts WHERE id=?1 AND agent_id=?2)",
            params![session.context_id.as_str(), session.agent_id.as_str()],
            |row| row.get(0),
        )
        .map_err(storage)?;
    if !known {
        return Err(GuidanceError::InvalidSession(
            "continuation context is not registered here".into(),
        ));
    }
    tx.execute(
        "DELETE FROM read_continuations WHERE expires_at <= unixepoch()",
        [],
    )
    .map_err(storage)?;
    if let Some(id) = tx
        .query_row(
            "SELECT id FROM read_continuations WHERE context_id=?1 AND fingerprint=?2",
            params![session.context_id.as_str(), fingerprint],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(storage)?
    {
        tx.commit().map_err(storage)?;
        return ReadContinuationId::parse(&id);
    }
    let id = RandomAgentIdentity::read_continuation()?;
    tx.execute(
        "INSERT INTO read_continuations(id,context_id,tool,arguments,fingerprint,expires_at)
         VALUES(?1,?2,?3,?4,?5,unixepoch()+86400)",
        params![
            id.as_str(),
            session.context_id.as_str(),
            call.tool,
            arguments,
            fingerprint
        ],
    )
    .map_err(storage)?;
    // Bounded transport state. Evidence pages themselves are never cached.
    tx.execute(
        "DELETE FROM read_continuations WHERE rowid IN
         (SELECT rowid FROM read_continuations WHERE context_id=?1 ORDER BY rowid DESC LIMIT -1 OFFSET 16)",
        [session.context_id.as_str()],
    ).map_err(storage)?;
    tx.execute(
        "DELETE FROM read_continuations WHERE rowid IN
         (SELECT rowid FROM read_continuations ORDER BY rowid DESC LIMIT -1 OFFSET 256)",
        [],
    )
    .map_err(storage)?;
    tx.commit().map_err(storage)?;
    Ok(id)
}

pub(super) fn load(
    directory: &SqliteAgentDirectory,
    id: &ReadContinuationId,
) -> Result<Option<ReadContinuation>, GuidanceError> {
    let connection = directory.connection.lock().map_err(storage)?;
    let row = connection.query_row(
        "SELECT tool,arguments FROM read_continuations WHERE id=?1 AND expires_at > unixepoch()",
        [id.as_str()], |row| Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?)),
    ).optional().map_err(storage)?;
    row.map(|(tool, arguments)| {
        ReadContinuation::new(&tool, serde_json::from_str(&arguments).map_err(storage)?)
    })
    .transpose()
}
