# TUI workflow

Running `cutlass` starts a Codex-style terminal conversation using the active
model configuration and the `Coder` agent preset:

```bash
cargo run
```

If there is no active model profile, Cutlass first opens an onboarding form.
It collects an OpenAI-compatible base URL, model ID, API key, and context
limits. Use `Tab` or the arrow keys to move between fields and `Ctrl+Enter` to
save. The API key is masked in the form and encrypted before it is written.

The header identifies the active model and agent. Conversation history fills
the center of the screen, while the prompt composer remains anchored at the
bottom.

Controls:

- `Enter`: submit the prompt
- `Alt+Enter`: insert a newline
- `Up`/`Down`: scroll the transcript
- `PageUp`/`PageDown`: scroll faster
- `Ctrl+C`: exit

Raw mode and the alternate terminal screen are restored automatically on
normal exit and recoverable errors.

The framework-neutral frontend contract lives in `src/ui/`. Ratatui widgets,
input handling, theme, and terminal lifecycle code live separately in
`src/tui/`.
