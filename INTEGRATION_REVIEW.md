# Integration review (initial concurrent snapshot)

Independent reviewer found these issues to verify after workers finish:

1. runtime customJs preserved but not executed; implement explicit trusted-code activation, instance-local forma binding, diagnostics. Untrusted default stays safe.
2. Canonical global audio/speech cues attached to first clip; need independent global transport (including no clips).
3. Event machine dispatch rotates order; maintain document machine ordering and test shared-variable guards.
4. Core parsing ignores unknown fields while CLI rejects; centralize strict schema parsing to avoid data loss.
5. Native preview selected-clip only vs full runtime composition; align via EvaluationContext or label isolation clearly.

Initial cargo test --workspace while editor still being written reported private app::apply_value and app::project_node referenced from ecs.rs. Recheck after completion.

Browser execution/audio not yet verified. Node fake-DOM tests do not establish real direct-file SVG compatibility.
