
First do the following:

1. Reproduce functionalities and overall tool look from inspiration that are possible from the inspiration/ folder for TUI
2. Add steering mid agent turn, and esc will no longer escape the session, but signal end turn (both things are directly appliable from steering inbox)

The result for group tools is something like this:

```terminaloutput
    <Tool group name/title>
        |- Read <path> (X lines)
        |- Rendered tree at <path> (X results, Y depth)
```
Without an enclosing visual box. Every tool group should declare its desired colour.

Drop the prompt box for a sleek background box
```terminaloutput
Worked for x seconds
---
<Input>
---
model: <id> · source: <current_workspace>
```