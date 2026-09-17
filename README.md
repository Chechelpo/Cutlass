# Cutlass

Brother multi-step coding agent harness project to citra.

## Advantages over citra:
Cutlass provides some advantages over its brother: 

1. **Ratatui** provides a way more stable library for TUI.
2. **Performance** rust provides less execution time for heavy tools (ex.: Tree-aider tool). The overhead is still
predominantly the agent call tho, so don't expect it to be noticeable.
3. **Types** the main reason why I dropped python. Heavy per tool configs were awkward to implement.
4. **Structure** clearer structure than citra (specially in regards to workflows)
5. **System-specific sandbox impl** bwrap for linux and MacOs, windows runs a pseudo sandbox.

## Disadvantages over citra
Most of citra's features are not yet implemented. 

1. Citra does a better job at sandboxing, as it creates temporary copies of binaries and other utilities in order to avoid touching
system binaries as much as possible. Cutlass uses the host's binaries in ro-binds while keeping only the current workspace as a w-bind.
2. Citra's tool-set is far more complete (for now).
3. Citra does not require building (python vs. rust)