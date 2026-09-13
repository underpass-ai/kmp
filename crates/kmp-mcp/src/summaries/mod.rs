//! Where the store's memories stand with respect to their English search
//! summaries.
//!
//! A memory written before summaries existed, or written in a language the
//! kernel cannot reach from English, is found from an English question only
//! through a `summary_en` its writer attached. The kernel cannot write one —
//! that is the model's job, at the one moment the kernel admits a model — but
//! it can say exactly which memories need one, which carry one that will not
//! carry retrieval, and which carry one that stands and still singles nothing
//! out. This reads that off the store's own event log, the way `document`
//! reads an about: the latest write of every entry wins, and the earlier ones
//! stay readable because they are where "the summary predates this text" is
//! written.
//!
//! One rule, the writer's: an entry whose text does not lean to English and
//! has no valid summary owes one; an entry whose summary fails the lint owes
//! one whatever its language, and the faults travel with it in the lint's own
//! words; an English entry with no summary owes nothing — an English question
//! already reaches it.
//!
//! Above that floor the reading reports weakness signals, which are warnings
//! and never refusals. They live here rather than in the kernel's value
//! objects because none of them is a property of one summary: each is read
//! against the about around it.
//!
//! There is one reading. [`SummaryAudit`] is it; the doctor's count, the
//! terminal's list of what is owed and the agent's `kmp_summaries_audit` are
//! projections of the same pass, and each surface maps it into its own shape
//! at its own boundary.

mod about_lexicon;
mod audit_scope;
mod audited_summary;
mod bundle_entries;
mod entry_history;
mod entry_revision;
mod summary_audit;
mod summary_judgement;
mod summary_state;
mod summary_totals;
mod summary_weakness;

pub use audit_scope::AuditScope;
pub use audited_summary::AuditedSummary;
pub use summary_audit::SummaryAudit;
pub use summary_state::SummaryState;
pub use summary_totals::SummaryTotals;
pub use summary_weakness::SummaryWeakness;
