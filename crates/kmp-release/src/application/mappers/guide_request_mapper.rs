use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::application::dto::guide_source_about_dto::GuideSourceAboutDto;
use crate::application::dto::guide_source_dto::GuideSourceDto;
use crate::application::dto::guide_tool_dto::GuideToolDto;
use crate::domain::guide_position_timestamp::GuidePositionTimestamp;
use crate::domain::release_error::ReleaseError;

pub struct GuideRequestMapper;

impl GuideRequestMapper {
    pub fn map(
        source: &GuideSourceDto,
        tools: &[GuideToolDto],
    ) -> Result<Vec<GuideRequestDocumentDto>, ReleaseError> {
        if source.schema_version != 1 {
            return Err(ReleaseError::invalid(
                "editorial.json has an unsupported schema_version",
            ));
        }
        if source.abouts.len() != 2
            || source.abouts[0].audience != "agent"
            || source.abouts[1].audience != "person"
        {
            return Err(ReleaseError::invalid(
                "editorial.json must declare the agent guide before the person guide",
            ));
        }
        let mut requests = Vec::new();
        for about in &source.abouts {
            requests.push(Self::map_about(source, about, tools)?);
        }
        Ok(requests)
    }

    fn map_about(
        source: &GuideSourceDto,
        about: &GuideSourceAboutDto,
        tools: &[GuideToolDto],
    ) -> Result<GuideRequestDocumentDto, ReleaseError> {
        let mut entries = Vec::new();
        let mut evidence = Vec::new();
        for (index, entry) in about.entries.iter().enumerate() {
            if entry
                .example_title
                .as_deref()
                .is_some_and(|title| title.trim().is_empty())
            {
                return Err(ReleaseError::invalid(
                    "guide example_title must not be empty",
                ));
            }
            Self::push_entry(
                &mut entries,
                &mut evidence,
                &about.about,
                &about.audience,
                &source.guide_version,
                &source.observed_at,
                index + 1,
                &entry.id,
                &entry.kind,
                &entry.depth,
                &entry.text,
                &entry.evidence,
                entry.example_title.as_deref(),
                None,
            )?;
            if let Some(title) = &entry.guide_title {
                if title.trim().is_empty() || title.contains(['|', '\n', '`']) {
                    return Err(ReleaseError::invalid(
                        "guide_title must be a non-empty table label",
                    ));
                }
                entries.last_mut().expect("just appended")["metadata"]["guide_title"] =
                    json!(title);
            }
        }
        if about.audience == "agent" {
            for tool in tools {
                let verb = Self::tool_verb(&tool.name)?;
                if tool.description.trim().is_empty() {
                    return Err(ReleaseError::invalid(format!(
                        "cannot place live tool {:?} in the agent guide",
                        tool.name
                    )));
                }
                let sequence = entries.len() + 1;
                Self::push_entry(
                    &mut entries,
                    &mut evidence,
                    &about.about,
                    &about.audience,
                    &source.guide_version,
                    &source.observed_at,
                    sequence,
                    &format!("tool:{}", tool.name),
                    "reference",
                    "advanced",
                    &format!("LIVE TOOL {}. {}", tool.name, tool.description.trim()),
                    tool.description.trim(),
                    None,
                    Some((&tool.name, verb)),
                )?;
            }
        }
        let mut relations = about
            .relations
            .iter()
            .enumerate()
            .map(|(index, relation)| {
                json!({
                    "from": format!("{}:{}", about.about, relation.from),
                    "to": format!("{}:{}", about.about, relation.to),
                    "rel": relation.rel,
                    "class": relation.class,
                    "why": relation.why,
                    "evidence": relation.evidence,
                    "confidence": "high",
                    "sequence": index + 1,
                })
            })
            .collect::<Vec<_>>();
        if about.audience == "agent" {
            for tool in tools {
                relations.push(json!({
                    "from": format!("{}:tool:{}", about.about, tool.name),
                    "to": format!("{}:{}", about.about, Self::tool_verb(&tool.name)?),
                    "rel": "uses_background",
                    "class": "evidential",
                    "why": "The human-readable verb rule is grounded in this exact live MCP contract.",
                    "evidence": tool.description.trim(),
                    "confidence": "high",
                    "sequence": relations.len() + 1,
                }));
            }
        }
        if relations.iter().any(|relation| {
            relation["why"].as_str().is_none_or(str::is_empty)
                || relation["evidence"].as_str().is_none_or(str::is_empty)
        }) {
            return Err(ReleaseError::invalid(
                "every guide relation must carry both why and evidence",
            ));
        }
        let logical = json!({
            "about": about.about,
            "memory": {
                "dimensions": [
                    {"id": "timeline", "kind": "timeline", "title": "Guide order"},
                    {"id": format!("audience-{}", about.audience), "kind": "audience", "title": about.audience},
                    {"id": "depth-basic", "kind": "depth", "title": "Basic"},
                    {"id": "depth-advanced", "kind": "depth", "title": "Advanced"},
                ],
                "entries": entries,
                "relations": relations,
                "evidence": evidence,
            },
            "provenance": {
                "source_kind": "derived",
                "source_agent": "kmp-guide-builder",
                "observed_at": source.observed_at,
                "correlation_id": format!("guide:kmp:v{}", source.guide_version),
                "causation_id": "release:guide-sync",
            }
        });
        let compact = serde_json::to_vec(&logical).map_err(|error| {
            ReleaseError::invalid(format!("could not encode guide request: {error}"))
        })?;
        let mut hash = Sha256::new();
        hash.update(b"kmp.guide.asset-with-revision.v1\0");
        hash.update(compact);
        let digest = format!("{:x}", hash.finalize());
        let mut body = logical;
        // The overview exposes the whole guide's revision without making an
        // agent fetch every lesson merely to detect that any lesson changed.
        if let Some(entries) = body["memory"]["entries"].as_array_mut() {
            for entry in entries {
                entry["metadata"]["guide_revision"] = Value::String(digest.clone());
            }
        }
        body["idempotency_key"] = Value::String(format!(
            "ingest:guide-sync:{}:{}:{}",
            source.guide_version,
            about.audience,
            &digest[..20]
        ));
        Ok(GuideRequestDocumentDto { body })
    }

    #[allow(clippy::too_many_arguments)]
    fn push_entry(
        entries: &mut Vec<Value>,
        evidence_items: &mut Vec<Value>,
        about: &str,
        audience: &str,
        guide_version: &str,
        observed_at: &str,
        sequence: usize,
        suffix: &str,
        kind: &str,
        depth: &str,
        text: &str,
        evidence: &str,
        example_title: Option<&str>,
        generated_tool: Option<(&str, &str)>,
    ) -> Result<(), ReleaseError> {
        let position = GuidePositionTimestamp::from_sequence(sequence)?;
        let source_kind = if generated_tool.is_some() {
            "tools/list"
        } else {
            "editorial"
        };
        let entry_ref = format!("{about}:{suffix}");
        let mut metadata = json!({
            "audience": audience,
            "depth": depth,
            "guide_version": guide_version,
            "source": source_kind,
        });
        if let Some((tool, verb)) = generated_tool {
            metadata["tool_name"] = Value::String(tool.to_string());
            metadata["guide_ref"] = Value::String(format!("{about}:{verb}"));
        }
        if let Some(title) = example_title {
            metadata["example_title"] = Value::String(title.to_string());
        }
        entries.push(json!({
            "id": entry_ref,
            "kind": kind,
            "text": text,
            "coordinates": [
                {
                    "dimension": "timeline",
                    "scope_id": "timeline",
                    "sequence": sequence,
                    "occurred_at": position.as_str(),
                    "observed_at": position.as_str(),
                },
                {
                    "dimension": "audience",
                    "scope_id": format!("audience-{audience}"),
                    "sequence": sequence,
                },
                {
                    "dimension": "depth",
                    "scope_id": format!("depth-{depth}"),
                    "sequence": sequence,
                }
            ],
            "metadata": metadata,
        }));
        evidence_items.push(json!({
            "id": format!("evidence:{about}:{suffix}"),
            "supports": [entry_ref],
            "text": evidence,
            "source": format!("KMP {source_kind} guide source v{guide_version}"),
            "time": observed_at,
            "metadata": {"guide_version": guide_version, "audience": audience},
        }));
        Ok(())
    }

    fn tool_verb(name: &str) -> Result<&'static str, ReleaseError> {
        match name {
            "kmp_guide" => Ok("verb:guide"),
            "kmp_ingest" | "kmp_write_memory" | "kmp_relabel" => Ok("verb:write"),
            "kmp_wake" => Ok("verb:wake"),
            "kmp_ask" => Ok("verb:ask"),
            "kmp_relate" => Ok("verb:relate"),
            "kmp_goto" | "kmp_near" | "kmp_rewind" | "kmp_forward" => Ok("verb:time"),
            "kmp_trace" | "kmp_inspect" => Ok("verb:audit"),
            "kmp_view_open" | "kmp_view_apply_intent" | "kmp_view_get_state" => Ok("verb:view"),
            other => Err(ReleaseError::invalid(format!(
                "cannot place live tool {other:?} in the agent guide"
            ))),
        }
    }
}
