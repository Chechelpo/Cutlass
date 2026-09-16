# TUI workflow

Running Cutlass opens a terminal UI with two phases: configuration, then a
conversation with the selected workflow:

```bash
cargo run
```

## Configuration

With no saved profiles, the connection form opens automatically. It collects a
profile name, OpenAI-compatible base URL (including `/v1` when required by the
provider), model ID, API key, context limit, and output limit. `Tab`, `Shift+Tab`,
or the arrow keys move between fields. `F2` saves; `Ctrl+Enter` and Enter on the
last field also save. The key is masked and encrypted before persistence using
the existing model configuration store. Plaintext key input is cleared after
saving or abandoning the form.

With saved profiles, choose a connection with Up/Down and Enter. Press `n` to
create another profile. Saving a new connection or choosing an existing one
opens the workflow picker. Connection setup is local; it does not make a model
request. Authentication and endpoint errors are shown on the first prompt.

## Workflow selection and sessions

Choose a workflow and press Enter to create a fresh session. No workflow or
agent session is created during connection setup. `Esc` goes back to connections.

The executable catalog is `built_in_workflows()` in
`src/orchestrator/workflow/session.rs`. Each `WorkflowDefinition` contains a name,
description, and factory returning a `WorkflowSession`. Register another factory
there to expose another workflow in the picker; the TUI uses the same session
interface for every entry. `Basic` is currently the only implemented workflow
and uses the `Coder` agent preset. The older workflow metadata trait/registry
is separate from this executable catalog.

The header identifies the selected workflow, model, and workspace. History fills
the center of the screen, while the prompt composer remains anchored at the
bottom.

Each session owns its workflow on a background worker. Input, scrolling,
resizing, and the busy indicator remain responsive during model calls. Completed
messages and tool groups arrive after each model round; responses are not yet
streamed token by token. You can draft the next prompt while the current turn
runs, but it cannot be submitted until that turn finishes.

User text is literal. Assistant text and declared Markdown in tool titles,
bodies, or group headers support headings, emphasis, lists, links, and code.
Tool results are displayed together under the header declared by their group.
Provider errors appear above the composer, keeping the transcript available.
Connections time out after 10 seconds; a complete request has a 120-second limit.

Controls:

- `Enter`: submit the prompt
- `Alt+Enter`: insert a newline
- `Left`/`Right`, `Home`/`End`, `Backspace`/`Delete`: edit the prompt
- Paste: insert text, preserving newlines in the composer
- `Up`/`Down`: scroll the transcript
- `PageUp`/`PageDown`: scroll faster
- `Ctrl+End`: follow the newest output
- `Esc` when idle: end the session and return to the workflow picker
- `Ctrl+C`: exit, including while a provider request is pending

Ending a session discards its in-memory conversation. Connection profiles remain
saved. Session resume and interrupting a turn while keeping its session are not
implemented yet.

Raw mode and the alternate terminal screen are restored automatically on
normal exit and recoverable errors.

Logs go only to `<workspace>/logs/cutlass.log.YYYY-MM-DD`.

Framework-neutral render descriptions live in `src/ui_interface/chat.rs`.
`src/tui/` contains configuration, session/worker communication, text input,
Markdown interpretation, rendering, and terminal lifecycle code. It does not
reconstruct tool groups from flat provider messages.

Run the on-file tests with `cargo test`. TUI tests exercise profile validation,
encrypted persistence, masking, small terminals, Unicode editing, Markdown,
grouped rendering, and the session factory using a mock alternate workflow.
