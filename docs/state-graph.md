# Visual state graph

Open **Behaviors → State graph** in the editor and select a state machine.
The graph edits a staged copy: **Apply** validates and commits the complete draft;
**Discard** restores the project. Undo/redo operates on applied changes.

- Drag a state's body to move it. **Auto layout** arranges states deterministically,
  including disconnected states and cycles. Positions are editor layout, not runtime state.
- Drag the right-hand connector onto a state to create a transition. Self-transitions
  are supported. Escape or **Cancel connection** cancels an unfinished connection.
- Select a state to choose the initial state and edit its entry/exit actions.
- Select a transition to edit its trigger, guard, blending and actions. Transition
  order is significant: the first transition matching the event and guard wins.
- Guards select a project variable, comparison operator and typed JSON scalar value.
  Action controls cover property/variable assignment, clip playback and weighting,
  audio playback and event emission. Actions execute in list order.

The machine JSON remains available for advanced values and schema features that do
not have a dedicated control. Opening structured controls does not coerce or discard
those values. Apply reports validation errors without replacing the current project.

Graph layout is not exported as animation behavior; the state machine itself is
shared by editor preview, native runtime and browser runtime. Custom JavaScript is
not executed by these graph controls. See [isolated preview](isolated-preview.md)
for a separate browser-only workflow for untrusted project scripts.
