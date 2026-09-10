use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use super::random_agent_identity::RandomAgentIdentity;
use crate::guidance::{
    AgentContext, AgentContextId, AgentDirectory, AgentId, AgentIdentity, AgentIdentitySource,
    AgentOpen, AgentSession, AgentUse, GuidanceError, UseOutcome,
};

/// Transport metadata beside the store. It never enters recall or evidence.
pub(crate) struct SqliteAgentDirectory {
    connection: Mutex<Connection>,
    identities: Arc<dyn AgentIdentitySource>,
    durable: bool,
}

fn storage(error: impl std::fmt::Display) -> GuidanceError {
    GuidanceError::Unavailable(format!("agent directory: {error}"))
}

fn count(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

impl SqliteAgentDirectory {
    pub(crate) fn at(path: &Path) -> Result<Self, GuidanceError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(storage)?;
        }
        Self::from_connection(
            Connection::open(path).map_err(storage)?,
            true,
            Arc::new(RandomAgentIdentity),
        )
    }

    pub(crate) fn volatile() -> Result<Self, GuidanceError> {
        Self::from_connection(
            Connection::open_in_memory().map_err(storage)?,
            false,
            Arc::new(RandomAgentIdentity),
        )
    }

    fn from_connection(
        connection: Connection,
        durable: bool,
        identities: Arc<dyn AgentIdentitySource>,
    ) -> Result<Self, GuidanceError> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(storage)?;
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(storage)?;
        if version > 2 {
            return Err(storage(format!("unsupported schema version {version}")));
        }
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, registration_key TEXT NOT NULL UNIQUE);
            CREATE TABLE IF NOT EXISTS contexts (
                id TEXT PRIMARY KEY, agent_id TEXT NOT NULL REFERENCES agents(id),
                context_key TEXT NOT NULL, revision TEXT NOT NULL, UNIQUE(agent_id, context_key));
            CREATE TABLE IF NOT EXISTS deliveries (
                context_id TEXT NOT NULL REFERENCES contexts(id), revision TEXT NOT NULL,
                topic TEXT NOT NULL, count INTEGER NOT NULL, expanded INTEGER NOT NULL,
                PRIMARY KEY(context_id, revision, topic)); CREATE TABLE IF NOT EXISTS uses (
                context_id TEXT NOT NULL REFERENCES contexts(id), revision TEXT NOT NULL,
                tool TEXT NOT NULL, attempts INTEGER NOT NULL, rejected INTEGER NOT NULL,
                unknown INTEGER NOT NULL, PRIMARY KEY(context_id,revision,tool));
            PRAGMA user_version=2;").map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
            identities,
            durable,
        })
    }

    fn register(&self, tx: &Transaction<'_>, key: &str) -> Result<AgentIdentity, GuidanceError> {
        if key.trim().is_empty() {
            return Err(GuidanceError::InvalidSession(
                "registration_key must identify one logical agent registration".into(),
            ));
        }
        if let Some((id, name)) = tx
            .query_row(
                "SELECT id, name FROM agents WHERE registration_key=?1",
                [key],
                |row| Ok((row.get::<_, String>(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage)?
        {
            return Ok(AgentIdentity {
                id: AgentId::parse(&id)?,
                name,
            });
        }
        loop {
            let identity = self.identities.agent()?;
            // Both ID and display name are unique. Retry entropy collisions;
            // the transaction keeps concurrent registration keys idempotent.
            let inserted = tx
                .execute(
                    "INSERT OR IGNORE INTO agents(id,name,registration_key) VALUES(?1,?2,?3)",
                    params![identity.id.as_str(), identity.name, key],
                )
                .map_err(storage)?;
            if inserted == 1 {
                return Ok(identity);
            }
        }
    }

    fn new_context(
        &self,
        tx: &Transaction<'_>,
        agent: &AgentId,
        key: &str,
        revision: &str,
    ) -> Result<AgentSession, GuidanceError> {
        if key.trim().is_empty() {
            return Err(GuidanceError::InvalidSession(
                "context_key must identify one logical context reset".into(),
            ));
        }
        if let Some(id) = tx
            .query_row(
                "SELECT id FROM contexts WHERE agent_id=?1 AND context_key=?2",
                params![agent.as_str(), key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage)?
        {
            return Ok(AgentSession {
                agent_id: agent.clone(),
                context_id: AgentContextId::parse(&id)?,
            });
        }
        loop {
            let id = self.identities.context()?;
            if tx.execute("INSERT OR IGNORE INTO contexts(id,agent_id,context_key,revision) VALUES(?1,?2,?3,?4)", params![id.as_str(),agent.as_str(),key,revision]).map_err(storage)? == 1 {
                return Ok(AgentSession { agent_id: agent.clone(), context_id: id });
            }
        }
    }

    fn snapshot(
        &self,
        tx: &Connection,
        session: &AgentSession,
        revision: &str,
    ) -> Result<AgentContext, GuidanceError> {
        let (name, previous) = tx.query_row("SELECT a.name,c.revision FROM contexts c JOIN agents a ON a.id=c.agent_id WHERE c.id=?1 AND a.id=?2", params![session.context_id.as_str(),session.agent_id.as_str()], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).optional().map_err(storage)?.ok_or_else(|| GuidanceError::InvalidSession("agent/context pair is not registered here; resume with the exact pair returned by KMP".into()))?;
        let mut expanded = Vec::new();
        let mut served = Vec::new();
        let mut statement = tx.prepare("SELECT topic, expanded FROM deliveries WHERE context_id=?1 AND revision=?2 ORDER BY topic").map_err(storage)?;
        for row in statement
            .query_map(params![session.context_id.as_str(), revision], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
            })
            .map_err(storage)?
        {
            let (topic, visible) = row.map_err(storage)?;
            if visible {
                expanded.push(topic.clone());
            }
            served.push(topic);
        }
        let mut query = tx.prepare("SELECT tool,attempts,rejected,unknown FROM uses WHERE context_id=?1 AND revision=?2 ORDER BY tool").map_err(storage)?;
        let used = query
            .query_map(params![session.context_id.as_str(), revision], |row| {
                Ok(AgentUse {
                    tool: row.get(0)?,
                    attempts: count(row, 1)?,
                    rejected: count(row, 2)?,
                    unknown: count(row, 3)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        Ok(AgentContext {
            identity: AgentIdentity {
                id: session.agent_id.clone(),
                name,
            },
            session: session.clone(),
            guide_revision: revision.to_owned(),
            expanded,
            served,
            guide_changed: previous != revision,
            durable: self.durable,
            used,
        })
    }

    fn delivery(
        &self,
        session: &AgentSession,
        revision: &str,
        topic: &str,
        expand: bool,
    ) -> Result<AgentContext, GuidanceError> {
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        if self.snapshot(&tx, session, revision)?.guide_changed {
            return Err(GuidanceError::StaleGuide);
        }
        if expand {
            let in_scheme = crate::guidance::guide_scheme::TOPICS
                .iter()
                .any(|(key, _)| *key == topic);
            tx.execute("INSERT INTO deliveries(context_id,revision,topic,count,expanded) VALUES(?1,?2,?3,1,?4) ON CONFLICT(context_id,revision,topic) DO UPDATE SET count=count+1, expanded=excluded.expanded", params![session.context_id.as_str(),revision,topic,in_scheme]).map_err(storage)?;
        } else {
            tx.execute(
                "UPDATE deliveries SET expanded=0 WHERE context_id=?1 AND revision=?2 AND topic=?3",
                params![session.context_id.as_str(), revision, topic],
            )
            .map_err(storage)?;
        }
        let result = self.snapshot(&tx, session, revision)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }
}

impl AgentDirectory for SqliteAgentDirectory {
    fn context(&self, id: &AgentContextId) -> Result<AgentContext, GuidanceError> {
        let connection = self.connection.lock().map_err(storage)?;
        let (agent,revision) = connection.query_row("SELECT agent_id,revision FROM contexts WHERE id=?1", [id.as_str()], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).optional().map_err(storage)?.ok_or_else(|| GuidanceError::InvalidSession("context_id is not registered here; open kmp_guide and preserve its returned id".into()))?;
        self.snapshot(
            &connection,
            &AgentSession {
                agent_id: AgentId::parse(&agent)?,
                context_id: id.clone(),
            },
            &revision,
        )
    }

    fn record_use(
        &self,
        session: &AgentSession,
        revision: &str,
        tool: &str,
        outcome: UseOutcome,
    ) -> Result<AgentUse, GuidanceError> {
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let current: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM contexts WHERE id=?1 AND agent_id=?2 AND revision=?3)",
                params![
                    session.context_id.as_str(),
                    session.agent_id.as_str(),
                    revision
                ],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if !current {
            return Err(GuidanceError::StaleGuide);
        }
        tx.execute("INSERT INTO uses(context_id,revision,tool,attempts,rejected,unknown) VALUES(?1,?2,?3,1,?4,?5) ON CONFLICT(context_id,revision,tool) DO UPDATE SET attempts=attempts+1,rejected=rejected+excluded.rejected,unknown=unknown+excluded.unknown",params![session.context_id.as_str(),revision,tool,matches!(outcome,UseOutcome::Rejected),matches!(outcome,UseOutcome::Unknown)]).map_err(storage)?;
        let result = tx.query_row("SELECT tool,attempts,rejected,unknown FROM uses WHERE context_id=?1 AND revision=?2 AND tool=?3",params![session.context_id.as_str(),revision,tool],|row| Ok(AgentUse {tool:row.get(0)?,attempts:count(row,1)?,rejected:count(row,2)?,unknown:count(row,3)?})).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

    fn open(&self, request: &AgentOpen, revision: &str) -> Result<AgentContext, GuidanceError> {
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let session = match request {
            AgentOpen::Register { key } => {
                let identity = self.register(&tx, key)?;
                self.new_context(&tx, &identity.id, "registration", revision)?
            }
            AgentOpen::Resume { context } => {
                let agent = tx
                    .query_row(
                        "SELECT agent_id FROM contexts WHERE id=?1",
                        [context.as_str()],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(storage)?
                    .ok_or_else(|| {
                        GuidanceError::InvalidSession(
                            "context_id is not registered here; copy the context returned by KMP"
                                .into(),
                        )
                    })?;
                AgentSession {
                    agent_id: AgentId::parse(&agent)?,
                    context_id: context.clone(),
                }
            }
            AgentOpen::NewContext { agent, key } => {
                if key.trim().is_empty() {
                    return Err(GuidanceError::InvalidSession(
                        "context_key must identify one logical context reset".into(),
                    ));
                }
                let exists: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1)",
                        [agent.as_str()],
                        |row| row.get(0),
                    )
                    .map_err(storage)?;
                if !exists {
                    return Err(GuidanceError::InvalidSession("agent_id is not registered here; preserve the KMP identity when starting another context".into()));
                }
                // Registration owns its context key; caller keys have a namespace.
                self.new_context(&tx, agent, &format!("reset:{key}"), revision)?
            }
        };
        let result = self.snapshot(&tx, &session, revision)?;
        tx.execute(
            "UPDATE contexts SET revision=?1 WHERE id=?2",
            params![revision, session.context_id.as_str()],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

    fn served(
        &self,
        session: &AgentSession,
        revision: &str,
        topic: &str,
    ) -> Result<AgentContext, GuidanceError> {
        self.delivery(session, revision, topic, true)
    }
    fn fold(
        &self,
        session: &AgentSession,
        revision: &str,
        topic: &str,
    ) -> Result<AgentContext, GuidanceError> {
        self.delivery(session, revision, topic, false)
    }
}

#[cfg(test)]
#[path = "sqlite_agent_directory_tests.rs"]
mod tests;
