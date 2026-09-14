use super::{
    engine::{Key, ReadTx, Table},
    serdes::{decode, encode},
    store::EmbeddedKernelStore,
};
use kmp_domain::{
    PortError,
    consolidation::{
        ConsolidatedView, ConsolidationFuture, ConsolidationRead, ConsolidationReadStatus,
        ConsolidationSource, ConsolidationStore, ConsolidationWrite, consolidate,
    },
};
use sha2::{Digest, Sha256};

fn identity(about: &str, view: &str) -> Result<String, PortError> {
    if about.trim().is_empty() || view.trim().is_empty() || about.len() > 512 || view.len() > 512 {
        return Err(PortError::InvalidState(
            "about and view require 1..512 bytes".into(),
        ));
    }
    Ok(format!(
        "{:x}",
        Sha256::digest(encode("view identity", &(about, view))?)
    ))
}

fn head(tx: &dyn ReadTx, id: &str) -> Result<u64, PortError> {
    tx.get(Table::ConsolidationHeads, Key::Str(id))?
        .map(|raw| decode("view head", &raw))
        .transpose()
        .map(|v| v.unwrap_or(0))
}

fn revision(tx: &dyn ReadTx, id: &str, rev: u64) -> Result<Option<ConsolidatedView>, PortError> {
    tx.get(Table::ConsolidationViews, Key::Str2(id, &rev.to_string()))?
        .map(|raw| decode("view revision", &raw))
        .transpose()
}

impl ConsolidationStore for EmbeddedKernelStore {
    fn consolidation_sources(
        &self,
        about: String,
        refs: Vec<String>,
    ) -> ConsolidationFuture<'_, Vec<ConsolidationSource>> {
        Box::pin(async move {
            self.run(move |store| {
                let tx = store.begin_read()?;
                super::consolidation_source::capture(tx.as_ref(), &about, &refs)
            })
            .await
        })
    }

    fn write_consolidation(
        &self,
        command: ConsolidationWrite,
    ) -> ConsolidationFuture<'_, ConsolidatedView> {
        Box::pin(async move {
            self.run(move |store| {
                let id = identity(&command.about, &command.view)?;
                let bytes = encode("consolidation command", &command)?;
                if bytes.len() > 2_097_152 {
                    return Err(PortError::InvalidState(
                        "consolidation command exceeds 2 MiB".into(),
                    ));
                }
                let digest = format!("{:x}", Sha256::digest(bytes));
                let mut tx = store.begin_write()?;
                // Retry returns the historical acceptance, even if a source or the
                // view has moved since. The separate current read checks freshness.
                if let Some(raw) = tx.get(
                    Table::ConsolidationReceipts,
                    Key::Str2(&id, &command.idempotency_key),
                )? {
                    let (stored_digest, rev): (String, u64) = decode("view receipt", &raw)?;
                    if digest != stored_digest {
                        return Err(PortError::Conflict(
                            "idempotency key reused for a different consolidation".into(),
                        ));
                    }
                    return revision(tx.as_ref(), &id, rev)?.ok_or_else(|| {
                        PortError::InvalidState("view receipt lacks revision".into())
                    });
                }
                let actual = head(tx.as_ref(), &id)?;
                if actual != command.expect_revision {
                    return Err(PortError::Conflict(format!(
                        "view moved: expected {}, actual {actual}",
                        command.expect_revision
                    )));
                }
                let refs = command.sources.keys().cloned().collect::<Vec<_>>();
                let sources =
                    super::consolidation_source::capture(tx.as_ref(), &command.about, &refs)?;
                let seconds = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| PortError::InvalidState(e.to_string()))?
                    .as_secs();
                let seconds = i64::try_from(seconds)
                    .map_err(|_| PortError::InvalidState("clock out of range".into()))?;
                let view = consolidate(
                    &command,
                    sources,
                    kmp_domain::rfc3339_from_epoch_seconds(seconds),
                )?;
                tx.insert(
                    Table::ConsolidationViews,
                    Key::Str2(&id, &view.revision.to_string()),
                    &encode("view", &view)?,
                )?;
                tx.insert(
                    Table::ConsolidationHeads,
                    Key::Str(&id),
                    &encode("head", &view.revision)?,
                )?;
                tx.insert(
                    Table::ConsolidationReceipts,
                    Key::Str2(&id, &command.idempotency_key),
                    &encode("receipt", &(digest, view.revision))?,
                )?;
                tx.commit()?;
                Ok(view)
            })
            .await
        })
    }

    fn read_consolidation(
        &self,
        about: String,
        view: String,
        requested: Option<u64>,
    ) -> ConsolidationFuture<'_, ConsolidationRead> {
        Box::pin(async move {
            self.run(move |store| {
                let id = identity(&about, &view)?;
                let tx = store.begin_read()?;
                let rev = requested.unwrap_or(head(tx.as_ref(), &id)?);
                let Some(view) = revision(tx.as_ref(), &id, rev)? else {
                    return Ok(ConsolidationRead {
                        status: ConsolidationReadStatus::Missing,
                        changed_sources: vec![],
                        view: None,
                    });
                };
                if requested.is_some() {
                    return Ok(ConsolidationRead {
                        status: ConsolidationReadStatus::HistoricalAudit,
                        changed_sources: vec![],
                        view: Some(view),
                    });
                }
                let mut changed = Vec::new();
                // Work is bounded by this view's dependencies, not the store size.
                // Capture failures fail closed; infrastructure errors still propagate.
                for source in &view.sources {
                    match super::consolidation_source::capture(
                        tx.as_ref(),
                        &about,
                        std::slice::from_ref(&source.reference),
                    ) {
                        Ok(current) if current.first().is_some_and(|s| s.stamp == source.stamp) => {
                        }
                        Ok(_) | Err(PortError::InvalidState(_)) => {
                            changed.push(source.reference.clone())
                        }
                        Err(error) => return Err(error),
                    }
                }
                if !changed.is_empty() {
                    return Ok(ConsolidationRead {
                        status: ConsolidationReadStatus::Stale,
                        changed_sources: changed,
                        view: None,
                    });
                }
                Ok(ConsolidationRead {
                    status: ConsolidationReadStatus::Current,
                    changed_sources: vec![],
                    view: Some(view),
                })
            })
            .await
        })
    }
}

#[cfg(test)]
#[path = "consolidation_tests.rs"]
mod tests;
