use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use kmp_domain::{
    BundleNode, BundleRelationship, ContextEventStore, DimensionScopeMode, DimensionSelection,
    DimensionSelectionMode, EntryLabels, GraphNeighborhoodReader, KmpBundle, KmpMode,
    LabelSelector, MemoryAboutIndexReader, MemoryDimensionIdentity, MemoryRelationType,
    NodeDetailReader, NodeRelationshipReader, ProjectionWriter, ResolutionTier, SnapshotStore,
    TemporalCoordinate, TemporalMemoryTraversal, TemporalTraversalRequest, labels_by_entry,
};

use crate::ApplicationError;
use crate::commands::CommandApplicationService;
use crate::memory::{
    AskMemoryQuery, ExistingMemoryRefs, InspectMemoryQuery, InspectMemoryResult, InspectedEvidence,
    MemoryIngestCommand, MemoryIngestOutcome, MemoryRelabelCommand, MemoryRelabelOutcome,
    RelateMemoryQuery, TemporalMemoryQuery, TemporalMemoryResult, TraceMemoryQuery,
    VisualProjectionQuery, VisualProjectionResult, WakeMemoryQuery, build_visual_projection,
    crosses_abouts, translate_memory_ingest, translate_memory_relabel, validate_ref_token,
    validate_supplied_entry_ref, validate_supplied_member_ref,
};
use crate::queries::{
    ContextRenderOptions, EndpointHint, GetContextPathQuery, GetContextPathResult, GetContextQuery,
    GetContextResult, GetNodeDetailQuery, GetNodeRelationshipsQuery,
    MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH, QueryApplicationService, render_graph_bundle_with_options,
};

const MEMORY_EXISTING_REFS_LOOKUP_DEPTH: u32 = 1;

pub struct KernelMemoryApplicationService<G, D, S, E, W> {
    query_application: Arc<QueryApplicationService<G, D, S>>,
    command_application: Arc<CommandApplicationService<E, W>>,
}

impl<G, D, S, E, W> KernelMemoryApplicationService<G, D, S, E, W> {
    pub fn new(
        query_application: Arc<QueryApplicationService<G, D, S>>,
        command_application: Arc<CommandApplicationService<E, W>>,
    ) -> Self {
        Self {
            query_application,
            command_application,
        }
    }
}

impl<G, D, S, E, W> KernelMemoryApplicationService<G, D, S, E, W>
where
    G: GraphNeighborhoodReader + MemoryAboutIndexReader + NodeRelationshipReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
    E: ContextEventStore + Send + Sync,
    W: ProjectionWriter + Send + Sync,
{
    pub async fn ingest(
        &self,
        command: MemoryIngestCommand,
    ) -> Result<MemoryIngestOutcome, ApplicationError> {
        let mut existing = self.existing_memory_refs(&command.about).await?;
        // A relation that declares an equivalence across abouts must land
        // on a ref that exists somewhere; the translation refuses it unless
        // this read found it, and reads nothing for any other relation.
        for relation in &command.memory.relations {
            let Ok(relation_type) = MemoryRelationType::new(&relation.rel) else {
                continue;
            };
            let target_ref = relation.target_ref.trim().to_string();
            if !crosses_abouts(&command.about, relation, &relation_type, &target_ref) {
                continue;
            }
            match self
                .query_application
                .get_node_detail(GetNodeDetailQuery {
                    node_id: target_ref.clone(),
                })
                .await
            {
                Ok(_) => {
                    existing.foreign.insert(target_ref);
                }
                Err(ApplicationError::NotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let (update_context, mut outcome) = translate_memory_ingest(&command, &existing)?;
        if command.dry_run {
            outcome
                .warnings
                .push("dry_run=true; validated memory without writing to the kernel".to_string());
            return Ok(outcome);
        }

        let accepted = self
            .command_application
            .update_context(update_context)
            .await?;
        outcome.read_after_write_ready = true;
        outcome.warnings.extend(accepted.warnings);
        Ok(outcome)
    }

    /// Changes the labels one entry stands in without rewriting its text.
    /// Read like a write: the about's catalogue and the entry's coordinates
    /// come from the store, the translation refuses what only the caller
    /// can fix, and one `memory_relabel` change goes to the log.
    pub async fn relabel(
        &self,
        command: MemoryRelabelCommand,
    ) -> Result<MemoryRelabelOutcome, ApplicationError> {
        let existing = self.existing_memory_refs(&command.about).await?;
        let current = self
            .entry_coordinates(&command.about, &command.ref_id, &existing)
            .await?;
        let (update_context, mut outcome) =
            translate_memory_relabel(&command, &existing, &current)?;
        if command.dry_run {
            outcome.warnings.push(
                "dry_run=true; validated the relabel against the store without writing to the kernel"
                    .to_string(),
            );
            return Ok(outcome);
        }

        let accepted = self
            .command_application
            .update_context(update_context)
            .await?;
        outcome.read_after_write_ready = true;
        outcome.warnings.extend(accepted.warnings);
        Ok(outcome)
    }

    /// The coordinates an entry stands in now, read off its `contains_entry`
    /// edges — the same reading `kmp_inspect` returns as the raw record.
    async fn entry_coordinates(
        &self,
        about: &str,
        ref_id: &str,
        existing: &ExistingMemoryRefs,
    ) -> Result<Vec<TemporalCoordinate>, ApplicationError> {
        validate_supplied_entry_ref(about, "ref", ref_id).map_err(ApplicationError::Validation)?;
        if !existing.refs.contains(ref_id) {
            return Err(ApplicationError::NotFound(format!(
                "`{ref_id}` is not a memory of `{about}`"
            )));
        }
        let links = self
            .query_application
            .get_node_relationships(GetNodeRelationshipsQuery {
                node_id: ref_id.to_string(),
            })
            .await?;
        inspect_raw_coordinates(ref_id, Some(&links))
    }

    /// The abouts the memory currently indexes, for consumers that render an
    /// index of what can be recalled — a viewer's sidebar, a CLI listing.
    pub async fn list_abouts(&self) -> Result<Vec<String>, ApplicationError> {
        self.query_application.list_memory_abouts().await
    }

    pub async fn wake(&self, query: WakeMemoryQuery) -> Result<GetContextResult, ApplicationError> {
        let render_options = memory_render_options(
            query.token_budget,
            query.max_tier,
            KmpMode::ResumeFocused,
            EndpointHint::Neighborhood,
        );
        let dimensions = query.dimensions.resolve_current_about(&query.about);
        let result = self
            .memory_context(
                &query.about,
                &query.role,
                query.depth,
                &dimensions,
                &render_options,
            )
            .await?;
        apply_dimension_selection(result, &dimensions, &render_options)
    }

    pub async fn ask(&self, query: AskMemoryQuery) -> Result<GetContextResult, ApplicationError> {
        let render_options = memory_render_options(
            query.token_budget,
            query.max_tier,
            KmpMode::ReasonPreserving,
            EndpointHint::Neighborhood,
        );
        let dimensions = query.dimensions.resolve_current_about(&query.about);
        let result = self
            .memory_context(
                &query.about,
                "answerer",
                query.depth,
                &dimensions,
                &render_options,
            )
            .await?;
        apply_dimension_selection(result, &dimensions, &render_options)
    }

    pub async fn temporal(
        &self,
        query: TemporalMemoryQuery,
    ) -> Result<TemporalMemoryResult, ApplicationError> {
        let render_options = memory_render_options(
            query.token_budget,
            query.max_tier,
            KmpMode::ReasonPreserving,
            EndpointHint::Neighborhood,
        );
        let dimensions = query.dimensions.resolve_current_about(&query.about);
        let context = self
            .memory_context(
                &query.about,
                "temporal-reader",
                query.depth,
                &dimensions,
                &render_options,
            )
            .await?;
        let quality = context.rendered.quality.clone();
        let source_bundle = filter_bundle_by_memory_dimensions(&context.bundle, &dimensions)?;

        let request = TemporalTraversalRequest::new(query.direction, query.cursor)
            .with_axis(query.axis)
            .with_dimensions(dimensions.clone())
            .with_requested_dimensions(query.dimensions.clone())
            .with_window(query.window);
        let request = if let Some(limit_entries) = query.limit_entries {
            request.with_limit_entries(limit_entries)?
        } else {
            request
        };

        let traversal = TemporalMemoryTraversal::traverse(&source_bundle, &request)?;

        Ok(TemporalMemoryResult {
            traversal,
            source_bundle,
            include: query.include,
            quality,
        })
    }

    /// A range- and level-of-detail-aware read model for renderers.
    ///
    /// This stays on the application facade: storage never gains a viewer
    /// query and renderers never reach into an engine. The temporal move
    /// narrows by the selected clock first; bins, clusters and moment entries
    /// are then projected on a channel whose result is not a model prompt.
    pub async fn visual_projection(
        &self,
        query: VisualProjectionQuery,
    ) -> Result<VisualProjectionResult, ApplicationError> {
        let temporal_query = query.temporal_query()?;
        let temporal = self.temporal(temporal_query).await?;
        build_visual_projection(&query, temporal)
    }

    /// The neighbourhood `relate` reads: the same load as `ask` over the
    /// abouts the selection names or resolves. Where the facts fall in time,
    /// and what they have to do with each other, is read from the bundle
    /// afterwards; the store is asked for nothing it does not hold.
    pub async fn relate(
        &self,
        query: RelateMemoryQuery,
    ) -> Result<GetContextResult, ApplicationError> {
        let render_options = memory_render_options(
            query.token_budget,
            query.max_tier,
            KmpMode::ReasonPreserving,
            EndpointHint::Neighborhood,
        );
        let dimensions = query.dimensions.resolve_current_about(&query.about);
        let result = self
            .memory_context(
                &query.about,
                "relater",
                query.depth,
                &dimensions,
                &render_options,
            )
            .await?;
        apply_dimension_selection(result, &dimensions, &render_options)
    }

    pub async fn trace(
        &self,
        query: TraceMemoryQuery,
    ) -> Result<GetContextPathResult, ApplicationError> {
        self.validate_read_members(
            &query.about,
            &[("from", query.from.as_str()), ("to", query.to.as_str())],
        )
        .await?;
        self.query_application
            .get_context_path(GetContextPathQuery {
                root_node_id: query.from,
                target_node_id: query.to,
                role: query.role,
                subtree_depth: Some(0),
                render_options: ContextRenderOptions {
                    focus_node_id: None,
                    token_budget: (query.token_budget > 0).then_some(query.token_budget),
                    max_tier: Some(ResolutionTier::L2EvidencePack),
                    rehydration_mode: KmpMode::ReasonPreserving,
                    endpoint_hint: EndpointHint::FocusedPath,
                },
            })
            .await
    }

    pub async fn inspect(
        &self,
        query: InspectMemoryQuery,
    ) -> Result<InspectMemoryResult, ApplicationError> {
        self.validate_read_members(&query.about, &[("ref", query.ref_id.as_str())])
            .await?;
        let include_incoming = query.include_incoming;
        let include_outgoing = query.include_outgoing;
        let include_details = query.include_details;
        let detail = self
            .query_application
            .get_node_detail(GetNodeDetailQuery {
                node_id: query.ref_id.clone(),
            })
            .await?;

        // Evidence is part of Inspect's contract independently of whether the
        // caller asks to render the incoming links. Resolve the direct graph
        // once, then follow only typed evidence sources.
        let links = self
            .query_application
            .get_node_relationships(GetNodeRelationshipsQuery {
                node_id: query.ref_id.clone(),
            })
            .await?;
        let mut evidence = Vec::new();
        let supporting_refs = links
            .incoming
            .iter()
            .filter(|relationship| relationship.relationship_type == "supports")
            .map(|relationship| relationship.source_node_id.clone())
            .collect::<BTreeSet<_>>();
        for evidence_ref in supporting_refs {
            let evidence_detail = match self
                .query_application
                .get_node_detail(GetNodeDetailQuery {
                    node_id: evidence_ref,
                })
                .await
            {
                Ok(detail) => detail,
                // A stale edge must not make the inspected node disappear.
                // It is not evidence unless its typed source still exists.
                Err(ApplicationError::NotFound(_)) => continue,
                Err(error) => return Err(error),
            };
            if !is_memory_evidence_kind(&evidence_detail.node.node_kind) {
                continue;
            }
            evidence.push(InspectedEvidence {
                supports: projected_evidence_supports(&evidence_detail, &query.ref_id),
                detail: evidence_detail,
            });
        }
        let raw_coordinates = if query.include_raw {
            inspect_raw_coordinates(&query.ref_id, Some(&links))?
        } else {
            Vec::new()
        };

        Ok(InspectMemoryResult {
            detail,
            incoming: if include_incoming {
                links.incoming.clone()
            } else {
                Vec::new()
            },
            outgoing: if include_outgoing {
                links.outgoing.clone()
            } else {
                Vec::new()
            },
            evidence,
            raw_coordinates,
            include_details,
            include_raw: query.include_raw,
        })
    }

    async fn existing_memory_refs(
        &self,
        about: &str,
    ) -> Result<ExistingMemoryRefs, ApplicationError> {
        match self
            .query_application
            .get_context(GetContextQuery {
                root_node_id: about.to_string(),
                role: "memory".to_string(),
                // Existing-ref validation only needs direct structural memory edges:
                // anchor -> dimensions, anchor -> entries, and anchor -> evidence.
                // Full semantic traversal here grows with every writer relation and
                // makes repeated ingest progressively slower.
                depth: MEMORY_EXISTING_REFS_LOOKUP_DEPTH,
                requested_scopes: Vec::new(),
                render_options: ContextRenderOptions::default(),
            })
            .await
        {
            Ok(result) => Ok(existing_refs_from_bundle(&result.bundle)),
            Err(ApplicationError::NotFound(_)) => Ok(ExistingMemoryRefs::default()),
            Err(error) => Err(error),
        }
    }

    async fn validate_read_members(
        &self,
        about: &str,
        members: &[(&str, &str)],
    ) -> Result<(), ApplicationError> {
        let mut graph_members = Vec::new();
        for (path, member_ref) in members {
            validate_ref_token(path, member_ref).map_err(ApplicationError::Validation)?;
            if validate_supplied_member_ref(about, path, member_ref).is_err() {
                graph_members.push((*path, *member_ref));
            }
        }
        if graph_members.is_empty() {
            return Ok(());
        }

        let visible = match self
            .query_application
            .get_context(GetContextQuery {
                root_node_id: about.to_string(),
                role: "memory-boundary".to_string(),
                depth: MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH,
                requested_scopes: Vec::new(),
                render_options: ContextRenderOptions::default(),
            })
            .await
        {
            Ok(result) => bundle_node_ids(&result.bundle),
            Err(ApplicationError::NotFound(_)) => BTreeSet::new(),
            Err(error) => return Err(error),
        };
        for (path, member_ref) in graph_members {
            if !visible.contains(member_ref) {
                return Err(ApplicationError::Validation(format!(
                    "`{path}` `{member_ref}` does not belong to about `{about}`"
                )));
            }
        }
        Ok(())
    }

    async fn memory_context(
        &self,
        about: &str,
        role: &str,
        depth: u32,
        dimensions: &DimensionSelection,
        render_options: &ContextRenderOptions,
    ) -> Result<GetContextResult, ApplicationError> {
        let roots = self.memory_context_roots(about, dimensions).await?;
        let requested_scopes = requested_dimension_scopes(about, dimensions, &roots);
        let mut results = Vec::new();
        for root in &roots {
            results.push(
                self.query_application
                    .get_context(GetContextQuery {
                        root_node_id: root.clone(),
                        role: role.to_string(),
                        depth,
                        requested_scopes: requested_scopes.clone(),
                        render_options: render_options.clone(),
                    })
                    .await?,
            );
        }

        merge_context_results(results, render_options)
    }

    async fn memory_context_roots(
        &self,
        current_about: &str,
        selection: &DimensionSelection,
    ) -> Result<Vec<String>, ApplicationError> {
        if selection.scope_mode() != DimensionScopeMode::AllAbouts {
            return context_roots(current_about, selection);
        }

        let roots = if should_filter_all_abouts_by_dimensions(selection) {
            // Kinds from `include`, exact ids from `scope_ids`, keys and
            // values from the positive selectors: the index resolves any of
            // them, and the filter that follows reads them all. The union is
            // a superset of what the conjoined filter keeps, never less.
            let dimension_ids = index_dimension_ids(selection);
            self.query_application
                .list_memory_abouts_by_dimensions(&dimension_ids)
                .await?
        } else {
            self.query_application.list_memory_abouts().await?
        };

        let roots = prioritize_current_about(normalize_about_roots(roots), current_about);
        if roots.is_empty() {
            return Err(ApplicationError::NotFound(
                "no memory abouts found for ALL_ABOUTS scope".to_string(),
            ));
        }
        Ok(roots)
    }
}

fn bundle_node_ids(bundle: &KmpBundle) -> BTreeSet<String> {
    std::iter::once(bundle.root_node().node_id())
        .chain(bundle.neighbor_nodes().iter().map(BundleNode::node_id))
        .map(ToString::to_string)
        .collect()
}

fn projected_evidence_supports(
    evidence: &crate::queries::GetNodeDetailResult,
    inspected_ref: &str,
) -> Vec<String> {
    evidence
        .node
        .properties
        .get("payload_supports")
        .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .filter(|supports| !supports.is_empty())
        .unwrap_or_else(|| vec![inspected_ref.to_string()])
}

fn should_filter_all_abouts_by_dimensions(selection: &DimensionSelection) -> bool {
    selection.scope_mode() == DimensionScopeMode::AllAbouts
        && ((selection.mode() == DimensionSelectionMode::Only
            && !selection.dimensions().is_empty())
            || !selection.scope_ids().is_empty()
            || selection.selectors().iter().any(LabelSelector::is_positive))
}

fn index_dimension_ids(selection: &DimensionSelection) -> Vec<String> {
    let mut dimension_ids = Vec::new();
    if selection.mode() == DimensionSelectionMode::Only {
        dimension_ids.extend(selection.dimensions().iter().cloned());
    }
    dimension_ids.extend(selection.scope_ids().iter().cloned());
    dimension_ids.extend(selection.positive_selector_ids());
    dimension_ids
}

fn inspect_raw_coordinates(
    ref_id: &str,
    links: Option<&crate::queries::GetNodeRelationshipsResult>,
) -> Result<Vec<TemporalCoordinate>, ApplicationError> {
    let Some(links) = links else {
        return Ok(Vec::new());
    };

    let mut coordinates = Vec::new();
    for relationship in links.incoming.iter().chain(links.outgoing.iter()) {
        if relationship.relationship_type != "contains_entry"
            || relationship.target_node_id != ref_id
        {
            continue;
        }
        if let Some(coordinate) =
            TemporalCoordinate::from_relation_explanation(&relationship.explanation)?
        {
            coordinates.push(coordinate);
        }
    }

    Ok(coordinates)
}

fn memory_render_options(
    token_budget: u32,
    max_tier: Option<ResolutionTier>,
    rehydration_mode: KmpMode,
    endpoint_hint: EndpointHint,
) -> ContextRenderOptions {
    ContextRenderOptions {
        focus_node_id: None,
        token_budget: (token_budget > 0).then_some(token_budget),
        max_tier,
        rehydration_mode,
        endpoint_hint,
    }
}

fn requested_dimension_scopes(
    current_about: &str,
    selection: &DimensionSelection,
    context_roots: &[String],
) -> Vec<String> {
    if !selection.scope_ids().is_empty() {
        return requested_explicit_dimension_scopes(current_about, selection, context_roots);
    }

    match selection.mode() {
        DimensionSelectionMode::Only => match selection.scope_mode() {
            DimensionScopeMode::CurrentAbout => selection
                .dimensions()
                .iter()
                .filter_map(|dimension| namespaced_dimension_id(current_about, dimension))
                .collect(),
            DimensionScopeMode::Abouts => selection
                .abouts()
                .iter()
                .flat_map(|about| {
                    selection
                        .dimensions()
                        .iter()
                        .filter_map(|dimension| namespaced_dimension_id(about, dimension))
                        .collect::<Vec<_>>()
                })
                .collect(),
            DimensionScopeMode::AllAbouts => context_roots
                .iter()
                .flat_map(|about| {
                    selection
                        .dimensions()
                        .iter()
                        .filter_map(|dimension| namespaced_dimension_id(about, dimension))
                        .collect::<Vec<_>>()
                })
                .collect(),
        },
        DimensionSelectionMode::Except | DimensionSelectionMode::All => Vec::new(),
    }
}

fn requested_explicit_dimension_scopes(
    current_about: &str,
    selection: &DimensionSelection,
    context_roots: &[String],
) -> Vec<String> {
    let abouts = match selection.scope_mode() {
        DimensionScopeMode::CurrentAbout => vec![current_about.to_string()],
        DimensionScopeMode::Abouts => selection.abouts().iter().cloned().collect(),
        DimensionScopeMode::AllAbouts => context_roots.to_vec(),
    };

    abouts
        .iter()
        .flat_map(|about| {
            selection
                .scope_ids()
                .iter()
                .filter_map(|scope_id| resolve_dimension_scope_id(about, scope_id))
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn context_roots(
    current_about: &str,
    selection: &DimensionSelection,
) -> Result<Vec<String>, ApplicationError> {
    match selection.scope_mode() {
        DimensionScopeMode::Abouts if !selection.abouts().is_empty() => {
            Ok(selection.abouts().iter().cloned().collect())
        }
        DimensionScopeMode::CurrentAbout => Ok(vec![current_about.to_string()]),
        DimensionScopeMode::Abouts => Err(ApplicationError::Validation(
            "dimension scope ABOUTS requires at least one about".to_string(),
        )),
        DimensionScopeMode::AllAbouts => Err(ApplicationError::Validation(
            "dimension scope ALL_ABOUTS must be resolved through the memory about index"
                .to_string(),
        )),
    }
}

fn apply_dimension_selection(
    mut result: GetContextResult,
    dimensions: &DimensionSelection,
    render_options: &ContextRenderOptions,
) -> Result<GetContextResult, ApplicationError> {
    result.bundle = filter_bundle_by_memory_dimensions(&result.bundle, dimensions)?;
    result.rendered = render_graph_bundle_with_options(&result.bundle, render_options);
    Ok(result)
}

fn normalize_about_roots(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn prioritize_current_about(mut roots: Vec<String>, current_about: &str) -> Vec<String> {
    let current_about = current_about.trim();
    if current_about.is_empty() {
        return roots;
    }
    if let Some(position) = roots.iter().position(|root| root == current_about) {
        let root = roots.remove(position);
        roots.insert(0, root);
    }
    roots
}

fn filter_bundle_by_memory_dimensions(
    bundle: &KmpBundle,
    dimensions: &DimensionSelection,
) -> Result<KmpBundle, ApplicationError> {
    let mut included_node_ids = BTreeSet::from([bundle.root_node().node_id().to_string()]);
    let mut selected_entry_ids = BTreeSet::new();
    let node_kinds = bundle_node_kinds(bundle);
    let labels = labels_by_entry(bundle);

    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
    {
        if contains_entry_selected(relationship, dimensions, &labels) {
            included_node_ids.insert(relationship.source_node_id().to_string());
            included_node_ids.insert(relationship.target_node_id().to_string());
            selected_entry_ids.insert(relationship.target_node_id().to_string());
        }
    }

    for relationship in bundle.relationships().iter().filter(|relationship| {
        relationship.relationship_type() == "supports"
            && selected_entry_ids.contains(relationship.target_node_id())
            && node_kinds
                .get(relationship.source_node_id())
                .is_some_and(|kind| is_memory_evidence_kind(kind))
    }) {
        included_node_ids.insert(relationship.source_node_id().to_string());
    }

    let neighbor_nodes = bundle
        .neighbor_nodes()
        .iter()
        .filter(|node| included_node_ids.contains(node.node_id()))
        .cloned()
        .collect::<Vec<_>>();
    let relationships = bundle
        .relationships()
        .iter()
        .filter(|relationship| {
            if relationship.relationship_type() == "contains_entry" {
                return contains_entry_selected(relationship, dimensions, &labels);
            }
            included_node_ids.contains(relationship.source_node_id())
                && included_node_ids.contains(relationship.target_node_id())
        })
        .cloned()
        .collect::<Vec<_>>();
    let node_details = bundle
        .node_details()
        .iter()
        .filter(|detail| included_node_ids.contains(detail.node_id()))
        .cloned()
        .collect::<Vec<_>>();

    KmpBundle::new(
        bundle.root_node_id().clone(),
        bundle.role().clone(),
        bundle.root_node().clone(),
        neighbor_nodes,
        relationships,
        node_details,
        bundle.metadata().clone(),
    )
    .map_err(Into::into)
}

/// A `contains_entry` edge survives when its coordinate passes the
/// coordinate filters and the entry it points at passes every selector:
/// the first reads one coordinate, the second the entry's whole label map.
fn contains_entry_selected(
    relationship: &BundleRelationship,
    dimensions: &DimensionSelection,
    labels: &BTreeMap<String, EntryLabels>,
) -> bool {
    let explanation = relationship.explanation();
    let coordinate_passes = dimensions.includes_coordinate(
        explanation.dimension().unwrap_or_default(),
        explanation.scope_id().unwrap_or_default(),
    );
    coordinate_passes
        && (!dimensions.has_selectors()
            || dimensions.admits(
                labels
                    .get(relationship.target_node_id())
                    .unwrap_or(&EntryLabels::default()),
            ))
}

fn bundle_node_kinds(bundle: &KmpBundle) -> BTreeMap<&str, &str> {
    let mut node_kinds =
        BTreeMap::from([(bundle.root_node().node_id(), bundle.root_node().node_kind())]);
    for node in bundle.neighbor_nodes() {
        node_kinds.insert(node.node_id(), node.node_kind());
    }
    node_kinds
}

fn is_memory_evidence_kind(kind: &str) -> bool {
    matches!(kind, "memory_evidence" | "evidence")
}

fn existing_refs_from_bundle(bundle: &KmpBundle) -> ExistingMemoryRefs {
    let mut refs = BTreeSet::from([bundle.root_node().node_id().to_string()]);
    let mut dimensions = BTreeSet::new();
    let mut max_sequences = BTreeMap::new();

    let mut labels = BTreeSet::new();
    for node in bundle.neighbor_nodes() {
        refs.insert(node.node_id().to_string());
        if node.node_kind() == "memory_dimension" {
            dimensions.insert(node.node_id().to_string());
            if let Some(kind) = node.properties().get("dimension_kind") {
                let value = MemoryDimensionIdentity::parse(node.node_id())
                    .map(|identity| identity.dimension_id().to_string())
                    .unwrap_or_else(|| node.node_id().to_string());
                labels.insert((kind.clone(), value));
            }
        }
    }

    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
    {
        dimensions.insert(relationship.source_node_id().to_string());
        let explanation = relationship.explanation();
        if let (Some(dimension), Some(scope_id), Some(sequence)) = (
            explanation.dimension(),
            explanation.scope_id(),
            explanation.sequence(),
        ) {
            max_sequences
                .entry((dimension.to_string(), scope_id.to_string()))
                .and_modify(|current: &mut u32| *current = (*current).max(sequence))
                .or_insert(sequence);
        }
    }

    ExistingMemoryRefs {
        refs,
        dimensions,
        labels,
        max_sequences,
        foreign: BTreeSet::new(),
    }
}

fn merge_context_results(
    mut results: Vec<GetContextResult>,
    render_options: &ContextRenderOptions,
) -> Result<GetContextResult, ApplicationError> {
    let mut result = results.remove(0);
    if results.is_empty() {
        return Ok(result);
    }

    let mut node_ids = BTreeSet::from([result.bundle.root_node().node_id().to_string()]);
    let mut neighbor_nodes = result.bundle.neighbor_nodes().to_vec();
    for node in &neighbor_nodes {
        node_ids.insert(node.node_id().to_string());
    }

    let mut relationships = result.bundle.relationships().to_vec();
    let mut relationship_ids = relationships
        .iter()
        .map(relationship_key)
        .collect::<BTreeSet<_>>();
    let mut node_details = result.bundle.node_details().to_vec();
    let mut detail_ids = node_details
        .iter()
        .map(|detail| detail.node_id().to_string())
        .collect::<BTreeSet<_>>();

    for other in results {
        push_node(&mut neighbor_nodes, &mut node_ids, other.bundle.root_node());
        for node in other.bundle.neighbor_nodes() {
            push_node(&mut neighbor_nodes, &mut node_ids, node);
        }
        for relationship in other.bundle.relationships() {
            if relationship_ids.insert(relationship_key(relationship)) {
                relationships.push(relationship.clone());
            }
        }
        for detail in other.bundle.node_details() {
            if detail_ids.insert(detail.node_id().to_string()) {
                node_details.push(detail.clone());
            }
        }
    }

    result.bundle = KmpBundle::new(
        result.bundle.root_node_id().clone(),
        result.bundle.role().clone(),
        result.bundle.root_node().clone(),
        neighbor_nodes,
        relationships,
        node_details,
        result.bundle.metadata().clone(),
    )
    .map_err(ApplicationError::Domain)?;
    result.rendered = render_graph_bundle_with_options(&result.bundle, render_options);
    Ok(result)
}

fn push_node(
    neighbor_nodes: &mut Vec<BundleNode>,
    node_ids: &mut BTreeSet<String>,
    node: &BundleNode,
) {
    if node_ids.insert(node.node_id().to_string()) {
        neighbor_nodes.push(node.clone());
    }
}

fn relationship_key(relationship: &BundleRelationship) -> (String, String, String) {
    (
        relationship.source_node_id().to_string(),
        relationship.target_node_id().to_string(),
        relationship.relationship_type().to_string(),
    )
}

fn namespaced_dimension_id(about: &str, dimension: &str) -> Option<String> {
    MemoryDimensionIdentity::new(about, dimension)
        .ok()
        .map(|identity| identity.node_id())
}

fn resolve_dimension_scope_id(about: &str, scope_id: &str) -> Option<String> {
    let scope_id = scope_id.trim();
    if scope_id.is_empty() {
        return None;
    }
    MemoryDimensionIdentity::resolve(about, scope_id).map(|identity| identity.node_id())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kmp_domain::{BundleMetadata, CaseId, RelationExplanation, RelationSemanticClass, Role};

    use super::*;

    #[test]
    fn all_abouts_scope_requires_about_index_resolution() {
        let selection = DimensionSelection::all().with_all_about_scope();
        let error = context_roots("question:current", &selection)
            .expect_err("ALL_ABOUTS must not fall back to current about directly");

        assert!(matches!(
            error,
            ApplicationError::Validation(message)
                if message.contains("resolved through the memory about index")
        ));
    }

    #[test]
    fn requested_scopes_expands_all_abouts_from_indexed_roots() {
        let selection = DimensionSelection::only(["timeline"]).with_all_about_scope();
        let scopes = requested_dimension_scopes(
            "question:current",
            &selection,
            &["question:a".to_string(), "question:b".to_string()],
        );

        assert_eq!(
            scopes,
            vec![
                "about:question:a:dimension:timeline".to_string(),
                "about:question:b:dimension:timeline".to_string()
            ]
        );
    }

    #[test]
    fn requested_scopes_expand_explicit_scope_ids_against_selected_abouts() {
        let selection = DimensionSelection::only(["conversation"])
            .with_about_scope(["question:a", "question:b"])
            .with_scope_ids([
                "conversation:alpha",
                "about:question:b:dimension:conversation:beta",
            ]);
        let scopes = requested_dimension_scopes("question:current", &selection, &[]);

        assert_eq!(
            scopes,
            vec![
                "about:question:a:dimension:conversation:alpha".to_string(),
                "about:question:b:dimension:conversation:alpha".to_string(),
                "about:question:b:dimension:conversation:beta".to_string()
            ]
        );
    }

    #[test]
    fn bundle_filter_narrows_same_dimension_kind_by_exact_scope_id() {
        let bundle = scoped_conversation_bundle();
        let selection = DimensionSelection::only(["conversation"])
            .resolve_current_about("question:a")
            .with_scope_ids(["conversation:alpha"]);

        let filtered =
            filter_bundle_by_memory_dimensions(&bundle, &selection).expect("bundle should filter");
        let node_ids = filtered
            .neighbor_nodes()
            .iter()
            .map(|node| node.node_id())
            .collect::<Vec<_>>();
        let relationships = filtered
            .relationships()
            .iter()
            .map(|relationship| {
                (
                    relationship.source_node_id(),
                    relationship.target_node_id(),
                    relationship.relationship_type(),
                )
            })
            .collect::<Vec<_>>();

        assert!(node_ids.contains(&"about:question:a:dimension:conversation:alpha"));
        assert!(node_ids.contains(&"claim:alpha"));
        assert!(!node_ids.contains(&"about:question:a:dimension:conversation:beta"));
        assert!(!node_ids.contains(&"claim:beta"));
        assert_eq!(
            relationships,
            vec![(
                "about:question:a:dimension:conversation:alpha",
                "claim:alpha",
                "contains_entry"
            )]
        );
    }

    #[test]
    fn bundle_filter_only_pulls_support_sources_when_source_is_memory_evidence() {
        let bundle = scoped_conversation_bundle_with_supports();
        let selection = DimensionSelection::only(["conversation"])
            .resolve_current_about("question:a")
            .with_scope_ids(["conversation:alpha"]);

        let filtered =
            filter_bundle_by_memory_dimensions(&bundle, &selection).expect("bundle should filter");
        let node_ids = filtered
            .neighbor_nodes()
            .iter()
            .map(|node| node.node_id())
            .collect::<Vec<_>>();
        let relationships = filtered
            .relationships()
            .iter()
            .map(|relationship| {
                (
                    relationship.source_node_id(),
                    relationship.target_node_id(),
                    relationship.relationship_type(),
                )
            })
            .collect::<Vec<_>>();

        assert!(node_ids.contains(&"claim:alpha"));
        assert!(node_ids.contains(&"evidence:alpha"));
        assert!(!node_ids.contains(&"claim:beta"));
        assert_eq!(
            relationships,
            vec![
                (
                    "about:question:a:dimension:conversation:alpha",
                    "claim:alpha",
                    "contains_entry"
                ),
                ("evidence:alpha", "claim:alpha", "supports")
            ]
        );
    }

    #[test]
    fn normalize_about_roots_trims_sorts_and_deduplicates() {
        assert_eq!(
            normalize_about_roots(vec![
                " question:b ".to_string(),
                String::new(),
                "question:a".to_string(),
                "question:b".to_string(),
            ]),
            vec!["question:a".to_string(), "question:b".to_string()]
        );
    }

    /// A sweep is narrowed by dimension kind through `include` and by exact
    /// scope through `scope_ids`; either alone is enough to ask the index.
    #[test]
    fn a_sweep_is_narrowed_by_kinds_or_by_exact_scopes() {
        let by_kind = DimensionSelection::only(["incident"]).with_all_about_scope();
        assert!(should_filter_all_abouts_by_dimensions(&by_kind));
        let by_scope = DimensionSelection::all()
            .with_all_about_scope()
            .with_scope_ids(["incident:north-outage"]);
        assert!(should_filter_all_abouts_by_dimensions(&by_scope));
        let whole_sweep = DimensionSelection::all().with_all_about_scope();
        assert!(!should_filter_all_abouts_by_dimensions(&whole_sweep));
        let inside_one_about = DimensionSelection::only(["incident"]);
        assert!(!should_filter_all_abouts_by_dimensions(&inside_one_about));
    }

    #[test]
    fn prioritize_current_about_keeps_all_roots_but_moves_current_first() {
        assert_eq!(
            prioritize_current_about(
                vec![
                    "question:a".to_string(),
                    "question:current".to_string(),
                    "question:z".to_string(),
                ],
                "question:current",
            ),
            vec![
                "question:current".to_string(),
                "question:a".to_string(),
                "question:z".to_string()
            ]
        );
    }

    #[test]
    fn bundle_filter_reads_selectors_over_the_entry_not_the_coordinate() {
        use kmp_domain::LabelSelectorOperator;

        let bundle = scoped_conversation_bundle();
        let keeps = |selection: DimensionSelection| {
            filter_bundle_by_memory_dimensions(&bundle, &selection)
                .expect("bundle should filter")
                .relationships()
                .iter()
                .filter(|relationship| relationship.relationship_type() == "contains_entry")
                .map(|relationship| relationship.target_node_id().to_string())
                .collect::<Vec<_>>()
        };
        let selector = |key: &str, operator: LabelSelectorOperator, values: &[&str]| {
            LabelSelector::new(key, operator, values.iter().copied()).expect("selector")
        };

        assert_eq!(
            keeps(DimensionSelection::all().with_selectors([selector(
                "conversation",
                LabelSelectorOperator::In,
                &["conversation:alpha"]
            )])),
            vec!["claim:alpha"]
        );
        assert_eq!(
            keeps(DimensionSelection::all().with_selectors([selector(
                "conversation",
                LabelSelectorOperator::NotIn,
                &["conversation:alpha"]
            )])),
            vec!["claim:beta"]
        );
        assert!(
            keeps(DimensionSelection::all().with_selectors([selector(
                "conversation",
                LabelSelectorOperator::NotExists,
                &[]
            )]))
            .is_empty()
        );
        assert_eq!(
            keeps(DimensionSelection::all().with_selectors([selector(
                "conversation",
                LabelSelectorOperator::Exists,
                &[]
            )])),
            vec!["claim:alpha", "claim:beta"]
        );
    }

    #[test]
    fn all_abouts_index_reads_positive_selectors_and_never_the_except_list() {
        use kmp_domain::LabelSelectorOperator;

        let by_selector = DimensionSelection::all()
            .with_all_about_scope()
            .with_selectors([
                LabelSelector::new(
                    "incident",
                    LabelSelectorOperator::Exists,
                    Vec::<String>::new(),
                )
                .expect("selector"),
                LabelSelector::new("env", LabelSelectorOperator::In, ["prod"]).expect("selector"),
            ]);
        assert!(should_filter_all_abouts_by_dimensions(&by_selector));
        assert_eq!(index_dimension_ids(&by_selector), vec!["incident", "prod"]);

        let negative_only = DimensionSelection::except(["task"])
            .with_all_about_scope()
            .with_selectors([LabelSelector::new(
                "customer",
                LabelSelectorOperator::NotIn,
                ["acme"],
            )
            .expect("selector")]);
        assert!(!should_filter_all_abouts_by_dimensions(&negative_only));
        assert!(index_dimension_ids(&negative_only).is_empty());
    }

    fn scoped_conversation_bundle() -> KmpBundle {
        KmpBundle::new(
            CaseId::new("question:a").expect("case id should be valid"),
            Role::new("temporal-reader").expect("role should be valid"),
            BundleNode::new(
                "question:a",
                "question",
                "Question A",
                "Test question",
                "ACTIVE",
                Vec::new(),
                BTreeMap::new(),
            ),
            vec![
                memory_dimension_node("about:question:a:dimension:conversation:alpha"),
                memory_dimension_node("about:question:a:dimension:conversation:beta"),
                claim_node("claim:alpha"),
                claim_node("claim:beta"),
            ],
            vec![
                contains_entry(
                    "about:question:a:dimension:conversation:alpha",
                    "claim:alpha",
                    1,
                ),
                contains_entry(
                    "about:question:a:dimension:conversation:beta",
                    "claim:beta",
                    2,
                ),
                cross_scope_constraint("claim:beta", "claim:alpha"),
            ],
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("test bundle should be valid")
    }

    fn scoped_conversation_bundle_with_supports() -> KmpBundle {
        KmpBundle::new(
            CaseId::new("question:a").expect("case id should be valid"),
            Role::new("temporal-reader").expect("role should be valid"),
            BundleNode::new(
                "question:a",
                "question",
                "Question A",
                "Test question",
                "ACTIVE",
                Vec::new(),
                BTreeMap::new(),
            ),
            vec![
                memory_dimension_node("about:question:a:dimension:conversation:alpha"),
                memory_dimension_node("about:question:a:dimension:conversation:beta"),
                claim_node("claim:alpha"),
                claim_node("claim:beta"),
                evidence_node("evidence:alpha"),
            ],
            vec![
                contains_entry(
                    "about:question:a:dimension:conversation:alpha",
                    "claim:alpha",
                    1,
                ),
                contains_entry(
                    "about:question:a:dimension:conversation:beta",
                    "claim:beta",
                    2,
                ),
                supports("claim:beta", "claim:alpha"),
                supports("evidence:alpha", "claim:alpha"),
            ],
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("test bundle should be valid")
    }

    fn memory_dimension_node(node_id: &str) -> BundleNode {
        BundleNode::new(
            node_id,
            "memory_dimension",
            node_id,
            "Conversation scope",
            "ACTIVE",
            Vec::new(),
            BTreeMap::new(),
        )
    }

    fn claim_node(node_id: &str) -> BundleNode {
        BundleNode::new(
            node_id,
            "claim",
            node_id,
            "Claim",
            "ACTIVE",
            Vec::new(),
            BTreeMap::new(),
        )
    }

    fn evidence_node(node_id: &str) -> BundleNode {
        BundleNode::new(
            node_id,
            "memory_evidence",
            node_id,
            "Evidence",
            "ACTIVE",
            Vec::new(),
            BTreeMap::new(),
        )
    }

    fn contains_entry(scope_id: &str, target_node_id: &str, sequence: u32) -> BundleRelationship {
        BundleRelationship::new(
            scope_id,
            target_node_id,
            "contains_entry",
            RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension("conversation")
                .with_scope_id(scope_id)
                .with_sequence(sequence),
        )
    }

    fn cross_scope_constraint(source_node_id: &str, target_node_id: &str) -> BundleRelationship {
        BundleRelationship::new(
            source_node_id,
            target_node_id,
            "contextual_constraint",
            RelationExplanation::new(RelationSemanticClass::Constraint)
                .with_rationale("Off-scope relation must not leak through exact scope filtering.")
                .with_confidence("medium"),
        )
    }

    fn supports(source_node_id: &str, target_node_id: &str) -> BundleRelationship {
        BundleRelationship::new(
            source_node_id,
            target_node_id,
            "supports",
            RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("Support relation for scoped filtering.")
                .with_confidence("medium"),
        )
    }
}
