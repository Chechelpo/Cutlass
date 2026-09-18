//! Coding-workflow memory tools and role capability presets.
//!
//! A preset always installs the complete typed memory group.  Roles can read
//! every type, while the preset narrows which lifecycle actions each role may
//! perform.  The execution path checks the same capability list advertised in
//! the JSON schema, so restrictions are not prompt-only policy.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{
    ChatCompletionTool, FunctionDefinition, ToolCall, ToolResult,
};
use crate::tools::group::{ToolGroup, ToolGroupKind};
use crate::tools::tool::Tool;
use crate::ui_interface::chat::{RenderText, RenderToolCall};

use super::session_memory::{MemoryId, MemoryKind, MemoryLink, MemoryRelation, MemoryStatus};

/// Operations that may be granted independently to an agent role.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryAction {
    View,
    Add,
    Update,
    Remove,
    Satisfy,
    Reopen,
    Check,
    Record,
    Invalidate,
    Resolve,
    Discard,
    Set,
}

impl MemoryAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Add => "add",
            Self::Update => "update",
            Self::Remove => "remove",
            Self::Satisfy => "satisfy",
            Self::Reopen => "reopen",
            Self::Check => "check",
            Self::Record => "record",
            Self::Invalidate => "invalidate",
            Self::Resolve => "resolve",
            Self::Discard => "discard",
            Self::Set => "set",
        }
    }
}

/// Complete memory-group selection plus per-kind mutation capabilities.
#[derive(Clone)]
pub struct MemoryGroupPreset {
    name: String,
    capabilities: BTreeMap<MemoryKind, Vec<MemoryAction>>,
}

impl MemoryGroupPreset {
    /// Prompt contract shared by presets that install workflow memory.
    pub fn protocol() -> &'static str {
        r#"# Shared workflow memory

Use the typed memory tools as the durable task handoff. Store compact facts and
outcomes, never conversation transcripts. All record types form one graph:
requirements are refined by acceptance criteria, covered by TODOs, implemented
by changes, and verified by evidence; issues block the earliest record or phase
that can correct them. Use typed IDs in links and inspect backlinks before
changing or removing a record. Your preset's tool schemas are authoritative
about which lifecycle actions this role owns. A denied action must be routed to
the owning role rather than worked around."#
    }

    /// Construct a custom role preset.  `view` is added to every memory kind;
    /// unsupported action/kind combinations are rejected immediately.
    pub fn new(
        name: impl Into<String>,
        grants: impl IntoIterator<Item = (MemoryKind, Vec<MemoryAction>)>,
    ) -> Result<Self, String> {
        let mut capabilities = MemoryKind::ALL
            .into_iter()
            .map(|kind| (kind, vec![MemoryAction::View]))
            .collect::<BTreeMap<_, _>>();
        for (kind, actions) in grants {
            let supported = supported_actions(kind);
            let selected = capabilities.get_mut(&kind).unwrap();
            for action in actions {
                if !supported.contains(&action) {
                    return Err(format!(
                        "Action '{}' is not supported by {} memory",
                        action.as_str(),
                        kind.heading()
                    ));
                }
                if !selected.contains(&action) {
                    selected.push(action);
                }
            }
        }
        Ok(Self {
            name: name.into(),
            capabilities,
        })
    }

    /// Full lifecycle access for the general-purpose Coder agent.
    pub fn coding() -> Self {
        Self::new(
            "coding",
            MemoryKind::ALL
                .into_iter()
                .map(|kind| (kind, supported_actions(kind).to_vec())),
        )
        .unwrap()
    }

    /// Problem-definition ownership matching the exploration role.
    pub fn explorer() -> Self {
        Self::new(
            "explorer",
            [
                grant(
                    MemoryKind::Requirement,
                    &[
                        MemoryAction::Add,
                        MemoryAction::Update,
                        MemoryAction::Remove,
                    ],
                ),
                grant(
                    MemoryKind::AcceptanceCriterion,
                    &[
                        MemoryAction::Add,
                        MemoryAction::Update,
                        MemoryAction::Remove,
                    ],
                ),
                grant(MemoryKind::Scope, mutable()),
                grant(MemoryKind::Constraint, mutable()),
                grant(MemoryKind::Fact, mutable()),
                grant(
                    MemoryKind::Issue,
                    &[
                        MemoryAction::Add,
                        MemoryAction::Update,
                        MemoryAction::Resolve,
                        MemoryAction::Remove,
                    ],
                ),
                grant(MemoryKind::WorkingState, working_actions()),
                grant(MemoryKind::Checkpoint, &[MemoryAction::Set]),
            ],
        )
        .unwrap()
    }

    /// Planning ownership: executable TODOs and consequential decisions.
    pub fn planner() -> Self {
        Self::new(
            "planner",
            [
                grant(MemoryKind::Todo, &[MemoryAction::Add, MemoryAction::Remove]),
                grant(MemoryKind::Decision, mutable()),
                grant(MemoryKind::Fact, mutable()),
                grant(
                    MemoryKind::Issue,
                    &[
                        MemoryAction::Add,
                        MemoryAction::Update,
                        MemoryAction::Resolve,
                        MemoryAction::Remove,
                    ],
                ),
                grant(MemoryKind::WorkingState, working_actions()),
                grant(MemoryKind::Checkpoint, &[MemoryAction::Set]),
            ],
        )
        .unwrap()
    }

    /// Implementation ownership: completing TODOs and recording actual deltas.
    pub fn implementer() -> Self {
        Self::new(
            "implementer",
            [
                grant(MemoryKind::Todo, &[MemoryAction::Check]),
                grant(
                    MemoryKind::Change,
                    &[
                        MemoryAction::Record,
                        MemoryAction::Update,
                        MemoryAction::Remove,
                    ],
                ),
                grant(MemoryKind::Fact, mutable()),
                grant(
                    MemoryKind::Issue,
                    &[
                        MemoryAction::Add,
                        MemoryAction::Update,
                        MemoryAction::Resolve,
                        MemoryAction::Reopen,
                    ],
                ),
                grant(MemoryKind::WorkingState, working_actions()),
                grant(MemoryKind::Checkpoint, &[MemoryAction::Set]),
            ],
        )
        .unwrap()
    }

    /// Independent verification ownership without implementation access.
    pub fn tester() -> Self {
        Self::new(
            "tester",
            [
                grant(
                    MemoryKind::Verification,
                    &[
                        MemoryAction::Record,
                        MemoryAction::Invalidate,
                        MemoryAction::Remove,
                    ],
                ),
                grant(
                    MemoryKind::Issue,
                    &[
                        MemoryAction::Add,
                        MemoryAction::Update,
                        MemoryAction::Resolve,
                        MemoryAction::Reopen,
                    ],
                ),
                grant(MemoryKind::WorkingState, working_actions()),
                grant(MemoryKind::Checkpoint, &[MemoryAction::Set]),
            ],
        )
        .unwrap()
    }

    /// Completion adjudication ownership for requirements and criteria.
    pub fn reviewer() -> Self {
        Self::new(
            "reviewer",
            [
                grant(
                    MemoryKind::Requirement,
                    &[MemoryAction::Satisfy, MemoryAction::Reopen],
                ),
                grant(
                    MemoryKind::AcceptanceCriterion,
                    &[MemoryAction::Satisfy, MemoryAction::Reopen],
                ),
                grant(
                    MemoryKind::Verification,
                    &[MemoryAction::Record, MemoryAction::Invalidate],
                ),
                grant(
                    MemoryKind::Issue,
                    &[
                        MemoryAction::Add,
                        MemoryAction::Update,
                        MemoryAction::Reopen,
                    ],
                ),
                grant(MemoryKind::WorkingState, working_actions()),
                grant(MemoryKind::Checkpoint, &[MemoryAction::Set]),
            ],
        )
        .unwrap()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn actions_for(&self, kind: MemoryKind) -> &[MemoryAction] {
        &self.capabilities[&kind]
    }

    /// Materialize one indivisible UI/tool group with a typed view for every
    /// memory kind.
    pub fn tool_group(&self) -> ToolGroup {
        let tools = MemoryKind::ALL
            .into_iter()
            .map(|kind| {
                Box::new(MemoryRecordTool::new(
                    kind,
                    self.capabilities[&kind].clone(),
                )) as Box<dyn crate::tools::tool::DynTool>
            })
            .collect();
        ToolGroup::new(ToolGroupKind::Memory, tools)
    }
}

fn grant(kind: MemoryKind, actions: &[MemoryAction]) -> (MemoryKind, Vec<MemoryAction>) {
    (kind, actions.to_vec())
}

fn mutable() -> &'static [MemoryAction] {
    &[
        MemoryAction::Add,
        MemoryAction::Update,
        MemoryAction::Remove,
    ]
}

fn issue_actions() -> &'static [MemoryAction] {
    &[
        MemoryAction::Add,
        MemoryAction::Update,
        MemoryAction::Resolve,
        MemoryAction::Reopen,
        MemoryAction::Remove,
    ]
}

fn working_actions() -> &'static [MemoryAction] {
    &[
        MemoryAction::Add,
        MemoryAction::Update,
        MemoryAction::Resolve,
        MemoryAction::Discard,
        MemoryAction::Remove,
    ]
}

fn supported_actions(kind: MemoryKind) -> &'static [MemoryAction] {
    match kind {
        MemoryKind::Requirement | MemoryKind::AcceptanceCriterion => &[
            MemoryAction::Add,
            MemoryAction::Update,
            MemoryAction::Satisfy,
            MemoryAction::Reopen,
            MemoryAction::Remove,
        ],
        MemoryKind::Scope | MemoryKind::Constraint | MemoryKind::Fact | MemoryKind::Decision => {
            mutable()
        }
        MemoryKind::Todo => &[
            MemoryAction::Add,
            MemoryAction::Update,
            MemoryAction::Check,
            MemoryAction::Reopen,
            MemoryAction::Remove,
        ],
        MemoryKind::Change => &[
            MemoryAction::Record,
            MemoryAction::Update,
            MemoryAction::Remove,
        ],
        MemoryKind::Verification => &[
            MemoryAction::Record,
            MemoryAction::Invalidate,
            MemoryAction::Remove,
        ],
        MemoryKind::Issue => issue_actions(),
        MemoryKind::WorkingState => working_actions(),
        MemoryKind::Checkpoint => &[MemoryAction::Set, MemoryAction::Remove],
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryInput {
    action: MemoryAction,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    status: Option<MemoryStatus>,
    #[serde(default)]
    links: Option<Vec<MemoryLinkInput>>,
    #[serde(default)]
    evidence: Option<String>,
    #[serde(default)]
    next_step: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryLinkInput {
    relation: MemoryRelation,
    target: String,
}

struct MemoryRecordTool {
    kind: MemoryKind,
    actions: Vec<MemoryAction>,
    id: String,
    name: String,
    description: String,
}

impl MemoryRecordTool {
    fn new(kind: MemoryKind, actions: Vec<MemoryAction>) -> Self {
        let name = kind.tool_name();
        Self {
            kind,
            id: format!("session_memory.{name}"),
            name,
            description: format!(
                "Read or modify {} in the shared workflow memory. Links are validated against all other memory types.",
                kind.heading().to_ascii_lowercase()
            ),
            actions,
        }
    }

    fn require_id(input: &MemoryInput, expected: MemoryKind) -> Result<MemoryId, String> {
        let raw = input
            .id
            .as_deref()
            .ok_or_else(|| "This action requires 'id'".to_string())?;
        let id = raw.parse::<MemoryId>()?;
        if id.kind != expected {
            return Err(format!(
                "Tool for {} cannot modify {id}",
                expected.heading()
            ));
        }
        Ok(id)
    }

    fn parse_links(input: Option<Vec<MemoryLinkInput>>) -> Result<Vec<MemoryLink>, String> {
        input
            .unwrap_or_default()
            .into_iter()
            .map(|link| {
                Ok(MemoryLink {
                    relation: link.relation,
                    target: link.target.parse()?,
                })
            })
            .collect()
    }

    fn render_success(call: &ToolCall, action: MemoryAction, value: Value) -> ToolResult {
        ToolResult::success(
            call,
            value,
            RenderToolCall::new(RenderText::markdown(format!(
                "**{}** workflow memory",
                action.as_str()
            ))),
        )
    }
}

impl Tool for MemoryRecordTool {
    type Action = MemoryAction;
    type Input = MemoryInput;
    type Config = ();

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn actions(&self) -> &[Self::Action] {
        &self.actions
    }

    fn deferred(&self) -> bool {
        false
    }

    fn execute(&self, call: &ToolCall, context: &AgentSession, input: Self::Input) -> ToolResult {
        if !self.actions.contains(&input.action) {
            return ToolResult::failure(
                call,
                format!(
                    "The '{}' memory preset does not allow action '{}' on {}",
                    context.preset.name,
                    input.action.as_str(),
                    self.kind.heading()
                ),
            );
        }
        let memory = &context.memory;
        let action = input.action;
        let result = (|| -> Result<Value, String> {
            match action {
                MemoryAction::View => Ok(json!({
                    "records": memory.records_of_kind(self.kind),
                    "completion_errors": memory.completion_errors(),
                })),
                MemoryAction::Set => {
                    if self.kind != MemoryKind::Checkpoint {
                        return Err("Only checkpoint memory supports 'set'".into());
                    }
                    let content = input
                        .content
                        .ok_or_else(|| "This action requires 'content'".to_string())?;
                    let next_step = input.next_step.ok_or_else(|| {
                        "Checkpoint action 'set' requires 'next_step'".to_string()
                    })?;
                    memory
                        .set_checkpoint(content, next_step)
                        .and_then(|change| {
                            serde_json::to_value(change).map_err(|error| error.to_string())
                        })
                }
                MemoryAction::Add | MemoryAction::Record => {
                    let content = input
                        .content
                        .ok_or_else(|| "This action requires 'content'".to_string())?;
                    let links = Self::parse_links(input.links)?;
                    memory
                        .add(self.kind, content, input.status, links, input.evidence)
                        .and_then(|change| {
                            serde_json::to_value(change).map_err(|error| error.to_string())
                        })
                }
                MemoryAction::Update => {
                    let id = Self::require_id(&input, self.kind)?;
                    let links = input
                        .links
                        .map(|links| Self::parse_links(Some(links)))
                        .transpose()?;
                    memory.update(id, input.content, links).and_then(|change| {
                        serde_json::to_value(change).map_err(|error| error.to_string())
                    })
                }
                MemoryAction::Remove => {
                    let id = Self::require_id(&input, self.kind)?;
                    memory.remove(id).and_then(|record| {
                        serde_json::to_value(record).map_err(|error| error.to_string())
                    })
                }
                transition => {
                    let id = Self::require_id(&input, self.kind)?;
                    let status = match transition {
                        MemoryAction::Satisfy => MemoryStatus::Satisfied,
                        MemoryAction::Reopen => MemoryStatus::Active,
                        MemoryAction::Check => MemoryStatus::Completed,
                        MemoryAction::Invalidate => MemoryStatus::Invalidated,
                        MemoryAction::Resolve => MemoryStatus::Resolved,
                        MemoryAction::Discard => MemoryStatus::Discarded,
                        _ => unreachable!(),
                    };
                    memory
                        .transition(id, status, input.evidence)
                        .and_then(|change| {
                            serde_json::to_value(change).map_err(|error| error.to_string())
                        })
                }
            }
        })();
        match result {
            Ok(value) => Self::render_success(call, action, value),
            Err(error) => ToolResult::failure(call, error),
        }
    }

    fn as_chat_completion_tool(&self) -> ChatCompletionTool {
        let actions = self
            .actions
            .iter()
            .map(|action| Value::String(action.as_str().into()))
            .collect::<Vec<_>>();
        ChatCompletionTool {
            tool_type: "function",
            function: FunctionDefinition {
                name: self.name.clone(),
                description: self.description.clone(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {"type": "string", "enum": actions},
                        "id": {"type": "string", "description": "Typed ID such as R1, TODO2, or CH1."},
                        "content": {"type": "string"},
                        "status": {
                            "type": "string",
                            "enum": ["active", "passed", "failed", "blocked"],
                            "description": "Required when recording verification; otherwise omit."
                        },
                        "links": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "relation": {
                                        "type": "string",
                                        "enum": ["refines", "covers", "implements", "verifies", "blocks", "derived_from", "supersedes", "related"]
                                    },
                                    "target": {"type": "string"}
                                },
                                "required": ["relation", "target"],
                                "additionalProperties": false
                            }
                        },
                        "evidence": {"type": "string", "description": "Evidence for satisfaction, completion, or resolution."},
                        "next_step": {"type": "string", "description": "Controller route selected by a checkpoint."}
                    },
                    "required": ["action"],
                    "additionalProperties": false
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_sees_every_memory_type_but_only_owned_actions() {
        let explorer = MemoryGroupPreset::explorer();
        assert_eq!(explorer.capabilities.len(), MemoryKind::ALL.len());
        assert!(
            explorer
                .actions_for(MemoryKind::Requirement)
                .contains(&MemoryAction::Add)
        );
        assert_eq!(
            explorer.actions_for(MemoryKind::Change),
            &[MemoryAction::View]
        );

        let reviewer = MemoryGroupPreset::reviewer();
        assert!(
            reviewer
                .actions_for(MemoryKind::Requirement)
                .contains(&MemoryAction::Satisfy)
        );
        assert!(
            !reviewer
                .actions_for(MemoryKind::Requirement)
                .contains(&MemoryAction::Update)
        );
    }

    #[test]
    fn rejects_capabilities_that_the_memory_kind_cannot_execute() {
        let error =
            MemoryGroupPreset::new("invalid", [(MemoryKind::Fact, vec![MemoryAction::Satisfy])])
                .err()
                .unwrap();
        assert!(error.contains("not supported"));
    }

    #[test]
    fn preset_materializes_as_one_complete_memory_group() {
        let group = MemoryGroupPreset::tester().tool_group();
        assert_eq!(group.kind(), ToolGroupKind::Memory);
        assert_eq!(group.tools().len(), MemoryKind::ALL.len());
        let verification = group
            .tools()
            .iter()
            .find(|tool| tool.name() == "memory_verification")
            .unwrap();
        let schema = serde_json::to_value(verification.as_chat_completion_tool()).unwrap();
        assert_eq!(
            schema["function"]["parameters"]["properties"]["action"]["enum"],
            json!(["view", "record", "invalidate", "remove"])
        );
    }

    #[test]
    fn role_schema_never_advertises_forbidden_actions() {
        let group = MemoryGroupPreset::reviewer().tool_group();
        let requirement = group
            .tools()
            .iter()
            .find(|tool| tool.name() == "memory_requirement")
            .unwrap();
        let schema = serde_json::to_value(requirement.as_chat_completion_tool()).unwrap();
        assert_eq!(
            schema["function"]["parameters"]["properties"]["action"]["enum"],
            json!(["view", "satisfy", "reopen"])
        );
    }
}
