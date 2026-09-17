use std::io;
use chrono::{DateTime, Local};
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::utils::directory_tree::render_tree;

pub enum UserPrependSections {
    RepoMap,
}

impl UserPrependSections {
    fn render(
        &self,
        sandboxed_filesystem: &SandboxedFilesystem,
    ) -> io::Result<String> {
        match self {
            Self::RepoMap => {
                render_tree(
                    sandboxed_filesystem.workspace_base(),
                    20,
                    false,
                    &[],
                    false,
                    200,
                    false,
                )
            }
        }
    }
}

pub fn build_user_context(
    user_sections: &[UserPrependSections],
    sandboxed_filesystem: &SandboxedFilesystem,
) -> io::Result<String> {
    user_sections
        .iter()
        .map(|section| section.render(sandboxed_filesystem))
        .collect::<io::Result<Vec<String>>>()
        .map(|sections| sections.join("\n"))
}


pub enum SysPromptSections{
    Environment,
    Workspace,
    CodingConventions,
    ToolGuides
}
impl SysPromptSections {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Environment,
            Self::Workspace,
            Self::ToolGuides,
            Self::CodingConventions,
        ]
    }
    fn render(&self, workspace: &SandboxedFilesystem) -> String {
        match self {
            Self::Environment => collect_environment(),

            Self::CodingConventions => {
                CODING_CONVENTIONS.to_string()
            }

            Self::Workspace => {
                format!(
                    "## Workspace\n\n- Current workspace: {}",
                    workspace.workspace_base().display()
                )
            },

            _ => "".to_string()
        }
    }
}

pub struct SysPrompt {
    preepend:String,
    std_sections: Vec<SysPromptSections>,
    append:String,
}

impl SysPrompt {
    pub fn new (preepend:String, std_sections:Vec<SysPromptSections>, append:String) -> Self {
        SysPrompt{
            preepend,
            std_sections,
            append,
        }
    }
    pub fn empty() -> Self {
        SysPrompt {
            preepend:String::from(""),
            std_sections:vec![],
            append: String::from("")
        }
    }

    pub fn build(&self, workspace:&SandboxedFilesystem) -> String {
        format!("{}\n{}\n{}", self.preepend, build_system_prompt(self, workspace), self.append)
    }
}

fn build_system_prompt(
    config: &SysPrompt,
    workspace: &SandboxedFilesystem,
) -> String {
    config
        .std_sections
        .iter()
        .map(|section| section.render(workspace))
        .collect::<Vec<String>>()
        .join("\n")
}

pub fn collect_environment() -> String {
    let now: DateTime<Local> = Local::now();

    let timezone = {
        let name = now.format("%Z").to_string();

        if !name.is_empty() {
            name
        } else {
            let offset = now.format("%z").to_string();

            if !offset.is_empty() {
                offset
            } else {
                "unknown".to_string()
            }
        }
    };

    format!(
        "## Environment\n\n\
         - OS: {} - {}\n\
         - Architecture: {}\n\
         - Date/time: {}\n\
         - Timezone: {}",
        std::env::consts::OS,
        "unknown",
        std::env::consts::ARCH,
        now.to_rfc3339_opts(
            chrono::SecondsFormat::Secs,
            true,
        ),
        timezone,
    )
}

const CODING_CONVENTIONS: &str = r#"
# Coding conventions
Here are some basic coding conventions you should follow for all your code

## Modules

When creating a module, whether it be a single or multi file module include the following:

    1. **Name of the module**
    2. **Date of creation**
    3. **Author: ** sign your edits/creation with your assigned name
    4. **Modification history:** edit/create sections with your assigned developer name
    5. **Synopsis: ** What this module does.
    6. **Global variables accessed or modified by the module**

This can be included directly in the source code in case of a single file module (ex.: api.py) or as the native module aggregator for the specific language
(ex.: package-info.java, __init__.py, etc.). In case there's no particular module aggregator and the module is a directory, document it on an AGENTS.md file.

Always prefer lowering the overall exports of a module. Try to keep the code that references this module routed through an interface or interface-like class.
Public-access members must be justified, the default is module-private.

## Naming conventions
When naming classes/variables/functions, follow these standards:

    1. Meaningful and understandable variables name helps anyone to understand the reason of using it.
    2. Local variables should be named using camel case lettering starting with small letter (e.g. localData) whereas Global variables names should start with a capital letter (e.g. GlobalData).
    Constant names should be formed using capital letters only (e.g. CONSDATA).
    3. It is better to avoid the use of digits in variable names.
    4. The names of the function should be written in the particular language coding conventions.
    5. The name of the function must describe the reason of using the function clearly and briefly.

## Logging

Your functions must provide logging with the following characteristics:

    1. **Level** (trace, debug, info, warn, error). Keep most logging at a debug level.
    2. **Thread** on multi-threading scenarios.
    3. **Source** add the source of this log into the message or, if possible, at the logger utility input.
    4. **Message** accurate message. Must include what's happening and variable values.

Investigate first for the existing logging utility. If there's none, invent your own. This utility must persist the latest log to disk.

## Function logic

Prefer:

    1. **Pure functions:** wherever possible, decompose functions into simpler, pure functions.
    2. **Indentation**
        - There must be a space after giving a comma between two function arguments
        - Each nested block should be properly indented and spaced. The maximum nest depth is 3. Anything else should use a sub-function.
        - Proper Indentation should be there at the beginning and at the end of each block in the program.
        - All braces should start from a new line and the code following the end of braces also start from a new line.
    3. **Early-exit** prefer checking params at the first lines of the function, with early returns. Avoid it if it'll make the code harder to read.

## Documentation
All functions/classes must contain documentation. For classes, its just what they're for and their data.

For functions, include:

    1. **Parameter information** list parameters and what they're expected to represent/how they'll be used etc. Name assumptions if any.
    2. **What it does** an explanation of what this function does.
    3. **Return** what this functions returns if any.
    4. **Exceptions** list out any expected exception of this function, along with the cases under which they may be thrown.

All global variables must have their purpose documented.

A developer must be able to understand all of the function without knowing the code. Keep in-line code comments updated to the logic.

"#;