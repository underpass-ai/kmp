use serde_json::Value;

/// What a native write answered, read the way a consumer has to read it before
/// it reads or scores anything.
///
/// `isError: false` only says the call was well formed. A preview
/// (`accepted: false`, `status: validated`) and a pending proposal
/// (`accepted: false`, `status: needs_review`) both come back that way, and
/// neither of them wrote anything. A harness that starts reading on that
/// signal scores a store that does not hold what it believes it seeded (#691).
#[derive(Debug, Clone)]
pub struct WriteReceipt {
    tool: String,
    status: Option<String>,
    accepted: Option<bool>,
    dry_run: bool,
    read_after_write_ready: Option<bool>,
    review: Option<Value>,
}

impl WriteReceipt {
    /// Reads the receipt out of a tool's `structuredContent`.
    pub fn read(tool: &str, content: &Value) -> Self {
        Self {
            tool: tool.to_string(),
            status: content["status"].as_str().map(str::to_string),
            accepted: content["accepted"].as_bool(),
            dry_run: content["dry_run"].as_bool().unwrap_or(false),
            // The canonical ingest has no review gate; what it answers instead
            // is whether the write is readable back.
            read_after_write_ready: content["memory"]["read_after_write_ready"].as_bool(),
            review: content
                .get("neighborhood")
                .filter(|neighborhood| !neighborhood.is_null())
                .cloned(),
        }
    }

    /// True only for a write the kernel says it committed or replayed.
    pub fn is_accepted(&self) -> bool {
        if self.dry_run {
            return false;
        }
        match self.accepted {
            // `unconfirmed` is not a commit: the kernel could not say the write
            // landed, so neither can its consumer.
            Some(accepted) => {
                accepted && matches!(self.status.as_deref(), Some("committed" | "replayed"))
            }
            None => self.read_after_write_ready == Some(true),
        }
    }

    /// True while the write is a proposal waiting on a review.
    pub fn needs_review(&self) -> bool {
        self.status.as_deref() == Some("needs_review")
    }

    /// The context the kernel served with a pending write, for the agent or
    /// person whose review it is. Reading it is not accepting it.
    pub fn review_context(&self) -> Option<&Value> {
        self.review.as_ref()
    }

    /// Refuses to let a caller continue on a write that did not land.
    ///
    /// This never resolves the review itself. Confirming a proposal is the
    /// writer's decision — a generic helper that made it would be recording an
    /// acceptance nobody gave.
    pub fn require_accepted(&self) -> Result<(), String> {
        if self.is_accepted() {
            return Ok(());
        }
        let status = self.status.as_deref().unwrap_or("(no status)");
        let detail = if self.dry_run {
            "it was a preview; a dry run writes nothing".to_string()
        } else if self.needs_review() {
            format!(
                "it is a proposal pending review{}; resolve it through the review this write \
                 returned, deliberately, before reading or scoring",
                match self.review_context() {
                    Some(review) => format!(
                        " over {} neighbors",
                        review["items"].as_array().map(Vec::len).unwrap_or_default()
                    ),
                    None => String::new(),
                }
            )
        } else if self.accepted.is_none() && self.read_after_write_ready.is_none() {
            "it carries no acceptance receipt at all".to_string()
        } else {
            "the kernel did not confirm it landed".to_string()
        };
        Err(format!(
            "`{}` did not commit: status `{status}` — {detail}. `isError: false` is not a receipt.",
            self.tool
        ))
    }
}
