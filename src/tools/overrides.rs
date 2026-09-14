use std::collections::HashSet;
use std::hash::Hash;
use crate::tools::tool::DynTool;

///
/// Declares a sub-set of the tool capacity
///
/// The idea is to have something like this.
/// ``` rust
/// #[derive(Debug, Deserialize)]
/// #[serde(rename_all = "lowercase")]
/// pub enum RequirementAction {
///     Add,
///     Update,
///     Remove,
///     Satisfy,
///     Reopen,
/// }
/// ```
/// Then override like this:
/// ```rust
/// let explorer_requirement_tool =
///     ToolOverride::new(
///         RequirementTool,
///         false,
///         [
///             RequirementAction::Add,
///             RequirementAction::Update,
///             RequirementAction::Remove,
///        ],
///     );
/// ```
pub struct ToolOverride<T, A>
where
    A: Eq + Hash,
{
    pub tool: T,
    pub allowed_actions: HashSet<A>,
    pub deferred:bool
}


impl<T, A> ToolOverride<T, A>
where
    A: Eq + Hash,
{
    pub fn new(
        tool: T,
        deferred: bool,
        actions: impl IntoIterator<Item = A>,
    ) -> Self {
        Self {
            tool,
            deferred,
            allowed_actions: actions.into_iter().collect(),
        }
    }
}