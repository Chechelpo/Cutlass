//! Shared, typed workflow memory.
//!
//! Memory belongs to a workflow conversation, not to a tool instance.  That
//! distinction lets a controller hand the same state to a fresh role while
//! keeping unrelated workflow runs isolated from one another.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::rc::Rc;
use std::str::FromStr;

use serde::{Deserialize, Serialize, Serializer};
use tracing::{debug, warn};

/// The durable record types that cooperate in one workflow memory graph.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Requirement,
    AcceptanceCriterion,
    Scope,
    Constraint,
    Fact,
    Decision,
    Todo,
    Change,
    Verification,
    Issue,
    WorkingState,
    Checkpoint,
}

impl MemoryKind {
    /// Every kind is always present in a memory-group preset.  Presets vary by
    /// mutation capability, never by hiding part of the shared state.
    pub const ALL: [Self; 12] = [
        Self::Requirement,
        Self::AcceptanceCriterion,
        Self::Scope,
        Self::Constraint,
        Self::Fact,
        Self::Decision,
        Self::Todo,
        Self::Change,
        Self::Verification,
        Self::Issue,
        Self::WorkingState,
        Self::Checkpoint,
    ];

    pub fn prefix(self) -> &'static str {
        match self {
            Self::Requirement => "R",
            Self::AcceptanceCriterion => "A",
            Self::Scope => "S",
            Self::Constraint => "C",
            Self::Fact => "F",
            Self::Decision => "D",
            Self::Todo => "TODO",
            Self::Change => "CH",
            Self::Verification => "V",
            Self::Issue => "I",
            Self::WorkingState => "W",
            Self::Checkpoint => "CP",
        }
    }

    pub fn heading(self) -> &'static str {
        match self {
            Self::Requirement => "Requirements",
            Self::AcceptanceCriterion => "Acceptance criteria",
            Self::Scope => "Scope",
            Self::Constraint => "Constraints",
            Self::Fact => "Facts",
            Self::Decision => "Decisions",
            Self::Todo => "TODOs",
            Self::Change => "Changes",
            Self::Verification => "Verification",
            Self::Issue => "Issues",
            Self::WorkingState => "Working state",
            Self::Checkpoint => "Checkpoint",
        }
    }

    pub fn tool_name(self) -> String {
        format!(
            "memory_{}",
            serde_json::to_value(self).unwrap().as_str().unwrap()
        )
    }
}

/// A stable, typed identifier.  Numeric sequences are local to each kind.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MemoryId {
    pub kind: MemoryKind,
    pub sequence: u64,
}

impl Display for MemoryId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}{}", self.kind.prefix(), self.sequence)
    }
}

impl Serialize for MemoryId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl FromStr for MemoryId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_uppercase();
        let (kind, suffix) = MemoryKind::ALL
            .iter()
            .copied()
            .map(|kind| (kind, kind.prefix()))
            .collect::<Vec<_>>()
            .into_iter()
            .filter(|(_, prefix)| normalized.starts_with(prefix))
            .max_by_key(|(_, prefix)| prefix.len())
            .map(|(kind, prefix)| (kind, &normalized[prefix.len()..]))
            .ok_or_else(|| format!("Unknown memory ID prefix in '{value}'"))?;
        let sequence = suffix
            .parse::<u64>()
            .map_err(|_| format!("Memory ID '{value}' must end with a positive integer"))?;
        if sequence == 0 {
            return Err(format!("Memory ID '{value}' must be positive"));
        }
        Ok(Self { kind, sequence })
    }
}

/// Lifecycle states shared by typed records.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Satisfied,
    Completed,
    Passed,
    Failed,
    Blocked,
    Resolved,
    Discarded,
    Invalidated,
}

impl Display for MemoryStatus {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let rendered = serde_json::to_value(self).unwrap();
        formatter.write_str(rendered.as_str().unwrap())
    }
}

/// Meaning carried by a directed link from one memory record to another.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRelation {
    Refines,
    Covers,
    Implements,
    Verifies,
    Blocks,
    DerivedFrom,
    Supersedes,
    Related,
}

impl Display for MemoryRelation {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let rendered = serde_json::to_value(self).unwrap();
        formatter.write_str(rendered.as_str().unwrap())
    }
}

/// A validated edge in the workflow memory graph.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryLink {
    pub relation: MemoryRelation,
    pub target: MemoryId,
}

/// One immutable snapshot of a durable memory record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryRecord {
    pub id: MemoryId,
    pub content: String,
    pub status: MemoryStatus,
    pub links: Vec<MemoryLink>,
    pub evidence: Option<String>,
    /// Controller route selected by the active role. Only checkpoints use it.
    pub next_step: Option<String>,
    pub revision: u64,
}

/// Result of a mutating operation, including records changed as a consequence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryChange {
    pub record: MemoryRecord,
    pub invalidated: Vec<MemoryId>,
}

#[derive(Default)]
struct MemoryState {
    records: BTreeMap<MemoryId, MemoryRecord>,
    next_ids: BTreeMap<MemoryKind, u64>,
}

/// Cloneable handle to the durable state shared by serial role sessions.
#[derive(Clone, Default)]
pub struct ConversationMemory {
    state: Rc<RefCell<MemoryState>>,
}

impl ConversationMemory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return defensive record snapshots in stable ID order.
    pub fn records(&self) -> Vec<MemoryRecord> {
        self.state.borrow().records.values().cloned().collect()
    }

    pub fn records_of_kind(&self, kind: MemoryKind) -> Vec<MemoryRecord> {
        self.state
            .borrow()
            .records
            .values()
            .filter(|record| record.id.kind == kind)
            .cloned()
            .collect()
    }

    pub fn get(&self, id: MemoryId) -> Option<MemoryRecord> {
        self.state.borrow().records.get(&id).cloned()
    }

    pub fn add(
        &self,
        kind: MemoryKind,
        content: impl Into<String>,
        status: Option<MemoryStatus>,
        links: Vec<MemoryLink>,
        evidence: Option<String>,
    ) -> Result<MemoryChange, String> {
        let content = non_empty(content.into(), "Memory content")?;
        let mut state = self.state.borrow_mut();
        let status = initial_status(kind, status)?;

        if kind == MemoryKind::Checkpoint {
            if let Some(existing) = state
                .records
                .values()
                .find(|record| record.id.kind == MemoryKind::Checkpoint)
                .cloned()
            {
                validate_links(&state, kind, &links, Some(existing.id))?;
                let record = MemoryRecord {
                    id: existing.id,
                    content,
                    status,
                    links,
                    evidence: normalize_optional(evidence),
                    next_step: None,
                    revision: existing.revision + 1,
                };
                state.records.insert(existing.id, record.clone());
                debug!(memory_id = %existing.id, revision = record.revision, "replaced workflow checkpoint");
                return Ok(MemoryChange {
                    record,
                    invalidated: Vec::new(),
                });
            }
        }
        validate_links(&state, kind, &links, None)?;
        let sequence = state.next_ids.entry(kind).or_insert(1);
        let id = MemoryId {
            kind,
            sequence: *sequence,
        };
        *sequence += 1;
        let record = MemoryRecord {
            id,
            content,
            status,
            links,
            evidence: normalize_optional(evidence),
            next_step: None,
            revision: 1,
        };
        state.records.insert(id, record.clone());
        debug!(memory_id = %id, kind = ?kind, "added workflow memory record");
        Ok(MemoryChange {
            record,
            invalidated: Vec::new(),
        })
    }

    /// Set the controller checkpoint while preserving its stable identity.
    pub fn set_checkpoint(
        &self,
        content: impl Into<String>,
        next_step: impl Into<String>,
    ) -> Result<MemoryChange, String> {
        let next_step = non_empty(next_step.into(), "Checkpoint next_step")?;
        let mut change = self.add(MemoryKind::Checkpoint, content, None, Vec::new(), None)?;
        let mut state = self.state.borrow_mut();
        let record = state.records.get_mut(&change.record.id).unwrap();
        record.next_step = Some(next_step);
        change.record = record.clone();
        Ok(change)
    }

    pub fn update(
        &self,
        id: MemoryId,
        content: Option<String>,
        links: Option<Vec<MemoryLink>>,
    ) -> Result<MemoryChange, String> {
        if content.is_none() && links.is_none() {
            return Err("Update requires 'content' and/or 'links'".into());
        }
        let mut state = self.state.borrow_mut();
        let current = require_record(&state, id)?.clone();
        let next_links = links.unwrap_or_else(|| current.links.clone());
        validate_links(&state, id.kind, &next_links, Some(id))?;
        let mut updated = current;
        if let Some(content) = content {
            updated.content = non_empty(content, "Memory content")?;
        }
        updated.links = next_links;
        updated.revision += 1;
        updated.evidence = None;
        if matches!(
            id.kind,
            MemoryKind::Requirement
                | MemoryKind::AcceptanceCriterion
                | MemoryKind::Todo
                | MemoryKind::Issue
                | MemoryKind::WorkingState
        ) {
            updated.status = MemoryStatus::Active;
        }
        state.records.insert(id, updated.clone());
        let invalidated = invalidate_dependants(&mut state, id);
        debug!(memory_id = %id, revision = updated.revision, invalidated = ?invalidated, "updated workflow memory record");
        Ok(MemoryChange {
            record: updated,
            invalidated,
        })
    }

    pub fn remove(&self, id: MemoryId) -> Result<MemoryRecord, String> {
        let mut state = self.state.borrow_mut();
        require_record(&state, id)?;
        let inbound = inbound_ids(&state, id);
        if !inbound.is_empty() {
            warn!(memory_id = %id, inbound = ?inbound, "rejected removal of referenced workflow memory");
            return Err(format!(
                "Cannot remove {id}; it is referenced by {}",
                join_ids(&inbound)
            ));
        }
        let removed = state.records.remove(&id).unwrap();
        debug!(memory_id = %id, "removed workflow memory record");
        Ok(removed)
    }

    pub fn transition(
        &self,
        id: MemoryId,
        status: MemoryStatus,
        evidence: Option<String>,
    ) -> Result<MemoryChange, String> {
        let mut state = self.state.borrow_mut();
        let current = require_record(&state, id)?.clone();
        validate_transition(&state, &current, status, evidence.as_deref())?;
        let mut updated = current;
        updated.status = status;
        updated.evidence = normalize_optional(evidence);
        updated.revision += 1;
        state.records.insert(id, updated.clone());
        let invalidated = if status == MemoryStatus::Active {
            invalidate_dependants(&mut state, id)
        } else {
            Vec::new()
        };
        debug!(memory_id = %id, status = %status, invalidated = ?invalidated, "transitioned workflow memory record");
        Ok(MemoryChange {
            record: updated,
            invalidated,
        })
    }

    /// Render all types together and include backlinks so a fresh role can see
    /// both what a record depends on and what depends on it.
    pub fn render(&self) -> String {
        let state = self.state.borrow();
        if state.records.is_empty() {
            return String::new();
        }
        let mut sections = Vec::new();
        for kind in MemoryKind::ALL {
            let records = state
                .records
                .values()
                .filter(|record| record.id.kind == kind)
                .collect::<Vec<_>>();
            if records.is_empty() {
                continue;
            }
            let mut lines = vec![format!("## {}", kind.heading())];
            for record in records {
                let outgoing = record
                    .links
                    .iter()
                    .map(|link| format!("{} {}", link.relation, link.target))
                    .collect::<Vec<_>>();
                let incoming = state
                    .records
                    .values()
                    .flat_map(|candidate| {
                        candidate
                            .links
                            .iter()
                            .filter(move |link| link.target == record.id)
                            .map(move |link| format!("{} from {}", link.relation, candidate.id))
                    })
                    .collect::<Vec<_>>();
                let mut suffixes = vec![format!("status: {}", record.status)];
                if !outgoing.is_empty() {
                    suffixes.push(format!("links: {}", outgoing.join(", ")));
                }
                if !incoming.is_empty() {
                    suffixes.push(format!("used by: {}", incoming.join(", ")));
                }
                if let Some(evidence) = &record.evidence {
                    suffixes.push(format!("evidence: {evidence}"));
                }
                if let Some(next_step) = &record.next_step {
                    suffixes.push(format!("next: {next_step}"));
                }
                lines.push(format!(
                    "- [{}] {} ({})",
                    record.id,
                    record.content,
                    suffixes.join("; ")
                ));
            }
            sections.push(lines.join("\n"));
        }
        sections.join("\n\n")
    }

    /// Enforce the completion gates used by serial workflows.
    pub fn completion_errors(&self) -> Vec<String> {
        let state = self.state.borrow();
        let mut errors = Vec::new();
        for record in state.records.values() {
            match (record.id.kind, record.status) {
                (MemoryKind::Requirement | MemoryKind::AcceptanceCriterion, status)
                    if status != MemoryStatus::Satisfied =>
                {
                    errors.push(format!("{} is not satisfied", record.id));
                }
                (MemoryKind::Todo, status) if status != MemoryStatus::Completed => {
                    errors.push(format!("{} is not completed", record.id));
                }
                (MemoryKind::Verification, MemoryStatus::Failed | MemoryStatus::Blocked) => {
                    errors.push(format!("{} is not passing", record.id));
                }
                (MemoryKind::Verification, MemoryStatus::Invalidated) => {
                    errors.push(format!("{} is stale", record.id));
                }
                (MemoryKind::Issue, status) if status != MemoryStatus::Resolved => {
                    errors.push(format!("{} is unresolved", record.id));
                }
                (MemoryKind::WorkingState, MemoryStatus::Active) => {
                    errors.push(format!("{} is still provisional", record.id));
                }
                _ => {}
            }
        }
        errors
    }

    pub fn is_resolved(&self) -> bool {
        self.completion_errors().is_empty()
    }
}

fn non_empty(value: String, label: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(format!("{label} cannot be empty"))
    } else {
        Ok(value)
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let item = item.trim().to_string();
        (!item.is_empty()).then_some(item)
    })
}

fn require_record(state: &MemoryState, id: MemoryId) -> Result<&MemoryRecord, String> {
    state
        .records
        .get(&id)
        .ok_or_else(|| format!("Memory record {id} does not exist"))
}

fn initial_status(
    kind: MemoryKind,
    requested: Option<MemoryStatus>,
) -> Result<MemoryStatus, String> {
    if kind == MemoryKind::Verification {
        let status = requested.ok_or_else(|| {
            "Verification records require status passed, failed, or blocked".to_string()
        })?;
        if matches!(
            status,
            MemoryStatus::Passed | MemoryStatus::Failed | MemoryStatus::Blocked
        ) {
            return Ok(status);
        }
        return Err("Verification status must be passed, failed, or blocked".into());
    }
    if let Some(status) = requested {
        if status != MemoryStatus::Active {
            return Err(format!("New {} records must start active", kind.heading()));
        }
    }
    Ok(MemoryStatus::Active)
}

fn validate_links(
    state: &MemoryState,
    source: MemoryKind,
    links: &[MemoryLink],
    source_id: Option<MemoryId>,
) -> Result<(), String> {
    let mut unique = BTreeSet::new();
    for link in links {
        if Some(link.target) == source_id {
            return Err("A memory record cannot link to itself".into());
        }
        require_record(state, link.target)?;
        if !unique.insert((link.relation as u8, link.target)) {
            return Err(format!(
                "Duplicate {} link to {}",
                link.relation, link.target
            ));
        }
        let target = link.target.kind;
        let valid = match link.relation {
            MemoryRelation::Refines => {
                source == MemoryKind::AcceptanceCriterion && target == MemoryKind::Requirement
            }
            MemoryRelation::Covers => {
                source == MemoryKind::Todo
                    && matches!(
                        target,
                        MemoryKind::Requirement | MemoryKind::AcceptanceCriterion
                    )
            }
            MemoryRelation::Implements => {
                source == MemoryKind::Change
                    && matches!(
                        target,
                        MemoryKind::Todo
                            | MemoryKind::Requirement
                            | MemoryKind::AcceptanceCriterion
                    )
            }
            MemoryRelation::Verifies => {
                source == MemoryKind::Verification
                    && matches!(
                        target,
                        MemoryKind::Change
                            | MemoryKind::Requirement
                            | MemoryKind::AcceptanceCriterion
                    )
            }
            MemoryRelation::Blocks => source == MemoryKind::Issue,
            MemoryRelation::DerivedFrom => {
                source != MemoryKind::WorkingState && target == MemoryKind::WorkingState
            }
            MemoryRelation::Supersedes => source == target,
            MemoryRelation::Related => true,
        };
        if !valid {
            return Err(format!(
                "{} records cannot use relation '{}' with {} records",
                source.heading(),
                link.relation,
                target.heading()
            ));
        }
    }
    Ok(())
}

fn validate_transition(
    state: &MemoryState,
    record: &MemoryRecord,
    target: MemoryStatus,
    evidence: Option<&str>,
) -> Result<(), String> {
    let allowed = match record.id.kind {
        MemoryKind::Requirement | MemoryKind::AcceptanceCriterion => {
            matches!(target, MemoryStatus::Satisfied | MemoryStatus::Active)
        }
        MemoryKind::Todo => matches!(target, MemoryStatus::Completed | MemoryStatus::Active),
        MemoryKind::Issue => matches!(target, MemoryStatus::Resolved | MemoryStatus::Active),
        MemoryKind::WorkingState => {
            matches!(
                target,
                MemoryStatus::Resolved | MemoryStatus::Discarded | MemoryStatus::Active
            )
        }
        MemoryKind::Verification => target == MemoryStatus::Invalidated,
        _ => false,
    };
    if !allowed {
        return Err(format!(
            "{} cannot transition to {target}",
            record.id.kind.heading()
        ));
    }

    if record.id.kind == MemoryKind::WorkingState {
        let promotions = inbound_links(state, record.id, MemoryRelation::DerivedFrom);
        if target == MemoryStatus::Resolved && promotions.is_empty() {
            return Err(format!(
                "Resolving {} requires at least one durable record linked with derived_from",
                record.id
            ));
        }
        if target == MemoryStatus::Discarded && !promotions.is_empty() {
            return Err(format!(
                "Discarding {} is not allowed after promotion by {}",
                record.id,
                join_ids(&promotions)
            ));
        }
        return Ok(());
    }

    let needs_evidence = matches!(target, MemoryStatus::Satisfied | MemoryStatus::Completed)
        || (record.id.kind == MemoryKind::Issue && target == MemoryStatus::Resolved);
    if needs_evidence && normalize_optional(evidence.map(str::to_owned)).is_none() {
        let has_passing_evidence = state.records.values().any(|candidate| {
            candidate.id.kind == MemoryKind::Verification
                && candidate.status == MemoryStatus::Passed
                && candidate.links.iter().any(|link| {
                    link.relation == MemoryRelation::Verifies && link.target == record.id
                })
        });
        if !has_passing_evidence {
            return Err(format!(
                "Transitioning {} to {target} requires evidence text or a passing verification link",
                record.id
            ));
        }
    }
    Ok(())
}

fn inbound_ids(state: &MemoryState, target: MemoryId) -> Vec<MemoryId> {
    state
        .records
        .values()
        .filter(|record| record.links.iter().any(|link| link.target == target))
        .map(|record| record.id)
        .collect()
}

fn inbound_links(state: &MemoryState, target: MemoryId, relation: MemoryRelation) -> Vec<MemoryId> {
    state
        .records
        .values()
        .filter(|record| {
            record
                .links
                .iter()
                .any(|link| link.target == target && link.relation == relation)
        })
        .map(|record| record.id)
        .collect()
}

fn invalidate_dependants(state: &mut MemoryState, changed: MemoryId) -> Vec<MemoryId> {
    let mut invalidated = Vec::new();
    let mut changed_targets = vec![changed];

    if changed.kind == MemoryKind::Requirement {
        let criteria = state
            .records
            .values_mut()
            .filter(|record| {
                record.id.kind == MemoryKind::AcceptanceCriterion
                    && record.links.iter().any(|link| {
                        link.relation == MemoryRelation::Refines && link.target == changed
                    })
            })
            .map(|record| {
                record.status = MemoryStatus::Active;
                record.evidence = None;
                record.revision += 1;
                record.id
            })
            .collect::<Vec<_>>();
        changed_targets.extend(criteria);
    }

    if changed.kind == MemoryKind::AcceptanceCriterion {
        let requirement_ids = state
            .records
            .get(&changed)
            .into_iter()
            .flat_map(|record| record.links.iter())
            .filter(|link| link.relation == MemoryRelation::Refines)
            .map(|link| link.target)
            .collect::<Vec<_>>();
        for requirement_id in requirement_ids {
            if let Some(requirement) = state.records.get_mut(&requirement_id) {
                requirement.status = MemoryStatus::Active;
                requirement.evidence = None;
                requirement.revision += 1;
                changed_targets.push(requirement_id);
            }
        }
    }

    let implementing_changes = state
        .records
        .values()
        .filter(|record| {
            record.id.kind == MemoryKind::Change
                && record.links.iter().any(|link| {
                    link.relation == MemoryRelation::Implements
                        && changed_targets.contains(&link.target)
                })
        })
        .map(|record| record.id)
        .collect::<Vec<_>>();
    changed_targets.extend(implementing_changes);
    changed_targets.sort_unstable();
    changed_targets.dedup();

    for record in state.records.values_mut() {
        if record.id.kind == MemoryKind::Verification
            && record.status != MemoryStatus::Invalidated
            && record.links.iter().any(|link| {
                link.relation == MemoryRelation::Verifies && changed_targets.contains(&link.target)
            })
        {
            record.status = MemoryStatus::Invalidated;
            record.evidence = Some(format!("Invalidated because {changed} changed"));
            record.revision += 1;
            invalidated.push(record.id);
        }
    }
    invalidated
}

fn join_ids(ids: &[MemoryId]) -> String {
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(relation: MemoryRelation, target: MemoryId) -> MemoryLink {
        MemoryLink { relation, target }
    }

    #[test]
    fn validates_the_typed_trace_and_renders_backlinks() {
        let memory = ConversationMemory::new();
        let requirement = memory
            .add(
                MemoryKind::Requirement,
                "Export reports",
                None,
                vec![],
                None,
            )
            .unwrap()
            .record;
        let criterion = memory
            .add(
                MemoryKind::AcceptanceCriterion,
                "CSV downloads",
                None,
                vec![link(MemoryRelation::Refines, requirement.id)],
                None,
            )
            .unwrap()
            .record;
        let todo = memory
            .add(
                MemoryKind::Todo,
                "Implement CSV endpoint",
                None,
                vec![link(MemoryRelation::Covers, criterion.id)],
                None,
            )
            .unwrap()
            .record;

        assert!(memory.render().contains("refines R1"));
        assert!(memory.render().contains("used by: covers from TODO1"));
        assert_eq!(todo.id.to_string(), "TODO1");
    }

    #[test]
    fn rejects_dangling_incompatible_and_inbound_removal() {
        let memory = ConversationMemory::new();
        let requirement = memory
            .add(
                MemoryKind::Requirement,
                "Keep API stable",
                None,
                vec![],
                None,
            )
            .unwrap()
            .record;
        assert!(
            memory
                .add(
                    MemoryKind::Fact,
                    "Not a criterion",
                    None,
                    vec![link(MemoryRelation::Refines, requirement.id)],
                    None,
                )
                .unwrap_err()
                .contains("cannot use relation")
        );
        memory
            .add(
                MemoryKind::Todo,
                "Preserve API",
                None,
                vec![link(MemoryRelation::Covers, requirement.id)],
                None,
            )
            .unwrap();
        assert!(memory.remove(requirement.id).unwrap_err().contains("TODO1"));
    }

    #[test]
    fn changing_a_requirement_reopens_criteria_and_invalidates_evidence() {
        let memory = ConversationMemory::new();
        let requirement = memory
            .add(MemoryKind::Requirement, "Original", None, vec![], None)
            .unwrap()
            .record;
        let criterion = memory
            .add(
                MemoryKind::AcceptanceCriterion,
                "Observable",
                None,
                vec![link(MemoryRelation::Refines, requirement.id)],
                None,
            )
            .unwrap()
            .record;
        let verification = memory
            .add(
                MemoryKind::Verification,
                "cargo test",
                Some(MemoryStatus::Passed),
                vec![link(MemoryRelation::Verifies, criterion.id)],
                None,
            )
            .unwrap()
            .record;
        memory
            .transition(criterion.id, MemoryStatus::Satisfied, None)
            .unwrap();
        memory
            .transition(
                requirement.id,
                MemoryStatus::Satisfied,
                Some("reviewed".into()),
            )
            .unwrap();

        let change = memory
            .update(requirement.id, Some("Clarified".into()), None)
            .unwrap();

        assert_eq!(change.invalidated, vec![verification.id]);
        assert_eq!(
            memory.get(requirement.id).unwrap().status,
            MemoryStatus::Active
        );
        assert_eq!(
            memory.get(criterion.id).unwrap().status,
            MemoryStatus::Active
        );
        assert_eq!(
            memory.get(verification.id).unwrap().status,
            MemoryStatus::Invalidated
        );
    }

    #[test]
    fn reports_cross_type_completion_gates() {
        let memory = ConversationMemory::new();
        let requirement = memory
            .add(MemoryKind::Requirement, "A requirement", None, vec![], None)
            .unwrap()
            .record;
        let todo = memory
            .add(
                MemoryKind::Todo,
                "Do it",
                None,
                vec![link(MemoryRelation::Covers, requirement.id)],
                None,
            )
            .unwrap()
            .record;
        assert_eq!(memory.completion_errors().len(), 2);
        memory
            .transition(
                requirement.id,
                MemoryStatus::Satisfied,
                Some("inspection".into()),
            )
            .unwrap();
        memory
            .transition(todo.id, MemoryStatus::Completed, Some("change CH1".into()))
            .unwrap();
        assert!(memory.is_resolved());
    }

    #[test]
    fn invalidates_verification_through_change_and_todo_links() {
        let memory = ConversationMemory::new();
        let requirement = memory
            .add(MemoryKind::Requirement, "Export data", None, vec![], None)
            .unwrap()
            .record;
        let todo = memory
            .add(
                MemoryKind::Todo,
                "Build export",
                None,
                vec![link(MemoryRelation::Covers, requirement.id)],
                None,
            )
            .unwrap()
            .record;
        let change = memory
            .add(
                MemoryKind::Change,
                "Added export.rs",
                None,
                vec![link(MemoryRelation::Implements, todo.id)],
                None,
            )
            .unwrap()
            .record;
        let verification = memory
            .add(
                MemoryKind::Verification,
                "export test passed",
                Some(MemoryStatus::Passed),
                vec![link(MemoryRelation::Verifies, change.id)],
                None,
            )
            .unwrap()
            .record;

        let updated = memory
            .update(todo.id, Some("Build CSV export".into()), None)
            .unwrap();

        assert_eq!(updated.invalidated, vec![verification.id]);
        assert_eq!(memory.get(todo.id).unwrap().status, MemoryStatus::Active);
        assert_eq!(
            memory.get(verification.id).unwrap().status,
            MemoryStatus::Invalidated
        );
    }

    #[test]
    fn checkpoint_replacement_preserves_identity_and_working_state_uses_promotions() {
        let memory = ConversationMemory::new();
        let first = memory
            .add(
                MemoryKind::Checkpoint,
                "Explore complete",
                None,
                vec![],
                None,
            )
            .unwrap()
            .record;
        let working = memory
            .add(
                MemoryKind::WorkingState,
                "The parser may be shared",
                None,
                vec![],
                None,
            )
            .unwrap()
            .record;
        assert!(
            memory
                .transition(working.id, MemoryStatus::Resolved, None)
                .unwrap_err()
                .contains("derived_from")
        );
        memory
            .add(
                MemoryKind::Fact,
                "Both commands use Parser",
                None,
                vec![link(MemoryRelation::DerivedFrom, working.id)],
                None,
            )
            .unwrap();
        memory
            .transition(working.id, MemoryStatus::Resolved, None)
            .unwrap();
        let second = memory
            .add(MemoryKind::Checkpoint, "Plan next", None, vec![], None)
            .unwrap()
            .record;

        assert_eq!(first.id, second.id);
        assert_eq!(second.revision, 2);
        assert_eq!(memory.records_of_kind(MemoryKind::Checkpoint).len(), 1);
    }
}
