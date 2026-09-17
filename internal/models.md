---
name: models
last_updated: 2026-09-17
id: 01M2PXCQDS6JFP04C2JK1T6XMD
---

# Model assignments

Which model runs which thunderstorm role. A review comment names the model and
reasoning level it came from, so the record shows who found what.

## Roles

| Role                 | Claude Code   | Codex and Pi                                                  | OpenCode Go | Cursor |
| -------------------- | ------------- | ------------------------------------------------------------- | ----------- | ------ |
| Implementer          | Opus, Sonnet  | `5.6-sol` at low or medium, `5.6-luna` at high or above       | todo        | todo   |
| Reviewer             | Opus, Fable   | `5.6-terra` at high or above, `5.6-sol` at medium, `6-astra` at medium | todo | todo   |
| Adversarial reviewer | Opus, Fable   | `5.6-sol` at high, `6-astra` at medium                        | todo        | todo   |

OpenCode Go and Cursor have no assignments yet. Do not run a thunderstorm role
on either until this table names one.

## Picking within a row

Take the cheaper option first. Move up when the work has one of these
properties, not because the change feels important:

- The failure is ambiguous and the cause is not yet located.
- The change crosses a module boundary or alters released behavior.
- The issue carries `risk:high`.
- A cheaper model already ran and its output did not survive review.

## Rules

The implementer and the reviewer never share a run. A model reviewing its own
diff inherits the gap that produced the defect.

The two standard review passes should not both use the same model and level
when another assignment in the row is available. Two passes from one
configuration produce close to one pass of coverage.

The adversarial pass assumes the standard passes ran. Give it a configuration
tuned for finding what they missed rather than repeating them.

## Signature

Every review and revision comment ends with the model and reasoning level:

```text
— <model-id> · <reasoning-level>
```

Use the identifier the harness reports, not a friendly name. Commits carry no
signature.
