# Harness design and polish

Use this reference for visual direction, component decisions, and design review.

## Contents

- Quality bar
- Visual audit
- Composition and rhythm
- Type, symbols, and copy
- Surfaces, borders, and color
- Component patterns
- Motion and live state
- Responsive behavior
- State matrix
- Polish rubric
- Reference harnesses

## Quality bar

thndrs is a conversation with an editor attached. The transcript records work and decisions. The composer is the immediate action. Status and metadata only matter when they help the user predict what happens next.

A polished harness:

- reveals the current action in a glance
- remains spatially stable while output streams
- gives text room to breathe without wasting the viewport
- uses repetition deliberately across turns, tools, and prompts
- protects the draft and focus through interruptions
- feels finished in error, empty, and constrained states
- looks at home in the terminal instead of imitating a desktop dashboard

Judge the live frame, not the number of visual features. Extra panels, borders, colors, metrics, and animation often weaken the hierarchy.

## Visual audit

Inspect a current capture at full size and at thumbnail scale. Ask:

1. What attracts the eye first? Is that the next important action?
2. Can the transcript, active work, composer, and metadata be distinguished by silhouette alone?
3. Do left edges, text insets, markers, and popup anchors share a small set of columns?
4. Are blank rows creating useful grouping or accidental holes?
5. Does any border, label, icon, or color repeat information already conveyed by position?
6. Does streaming or a status change move the composer, selection, or unrelated transcript content?
7. Does the frame still read with color disabled and at a narrow width?

Fix composition and hierarchy before choosing new colors or glyphs.

## Composition and rhythm

Use four visual layers:

1. active decision or newest meaningful output
2. editable composer
3. conversation and tool history
4. status, session, model, path, token, and shortcut metadata

Allow the active state to reorder those layers. A permission prompt or picker can become primary while it owns focus.

Keep a small spacing vocabulary. Prefer consistent content insets, one standard gap between related rows, and a larger gap between separate turns or sections. Align recurring markers, tool names, timestamps, and message text. Uneven padding reads as a rendering defect even when every row is technically correct.

Use the viewport as one composition. The transcript should absorb available height. The composer should remain anchored, and transient surfaces should appear near the control or content that invoked them. Avoid permanent rows for rare information.

## Type, symbols, and copy

Terminal typography comes from weight, dimness, case, symbols, whitespace, and alignment.

- Use normal weight for reading and bold for short active labels or decisions.
- Dim metadata only when it remains readable on light and dark backgrounds.
- Keep headings and labels in sentence case unless an established command or acronym requires otherwise.
- Choose a small symbol vocabulary and keep each symbol's meaning stable.
- Prefer familiar ASCII or common-width glyphs for structural markers. Test every decorative Unicode symbol in target terminals.
- Write status text as a current fact or available action: `Running tests`, `Waiting for approval`, `Esc to cancel`.
- Remove internal state names, test language, and renderer terminology from product copy.
- Truncate paths and identifiers around their meaningful parts. Do not let metadata dominate because it is long.

Place shortcut hints close to the action they affect. Keep the full key vocabulary in help or a command palette.

## Surfaces, borders, and color

Use visual depth sparingly:

- terminal background for transcript, labels, surrounding space, and footer;
- one filled input surface for editable composer rows;
- a focused selection surface for a picker, permission choice, or modal;
- overlays only when focus and dismissal are unambiguous.

Prefer whitespace, indentation, and a semantic marker before drawing a border. A border earns its cells when it defines input bounds, focus, selection, or modal scope. Avoid stacked boxes and full-width rules that split a continuous conversation into dashboard panels.

Style by meaning:

| Role      | Use                                    | Treatment                               |
| --------- | -------------------------------------- | --------------------------------------- |
| Primary   | conversation and active choice         | terminal-default foreground             |
| Secondary | metadata and supporting detail         | dim or muted semantic color             |
| Focus     | active input or selection              | one accent plus a non-color cue         |
| Success   | completion or addition                 | green-family accent plus text or symbol |
| Warning   | recoverable risk or required choice    | warm accent plus the next action        |
| Error     | failure or deletion                    | red-family accent plus an explanation   |
| Agent     | assistant identity or active reasoning | restrained product accent               |
| Tool      | command, file, or process activity     | quiet semantic marker and outcome       |

Start with terminal-default foreground and background. If themes use RGB colors, centralize them, detect capability, quantize deliberately, and retain a terminal-native/no-color fallback. Check actual foreground/background pairs on light and dark terminals.

## Component patterns

### Transcript

- Keep prose wide enough to read and narrow enough to scan.
- Separate turns through spacing and a stable author or role cue.
- Visually compress routine successful tool work; expand failures, requested detail, and current activity.
- Preserve a path to truncated output.
- Distinguish user, agent, tool, and system messages without wrapping every type in a unique box.
- When the user scrolls away from the tail, show that new output exists without stealing the viewport.

### Composer

- Pin it to the bottom and grow it upward.
- Fill only editable rows and intentional inset padding with `panel_bg`.
- Keep mode, validation, queue, and disabled states inside or directly adjacent to the input surface.
- Make placeholder text quieter than a draft.
- Keep the cursor visible and clamped after wrapping, resize, completion, and multiline edits.
- Preserve the draft until a successful action consumes it.

### Tools and progress

- Use a stable leading marker and concise verb for the current action.
- Group related calls under one Activity rail, align their markers, outcomes, names, and metadata, and keep live output visible beneath the running call.
- Render requested tool disclosure inline beneath its originating transcript entry. Show the disclosure key only on the current eligible entry; never replace the composer accessory area with transcript detail.
- Keep spinner width constant so adjacent text does not move.
- Replace transient progress with a durable outcome when work completes.
- Show duration, exit status, or output counts only when they help interpret the result.
- Put recovery or approval actions directly after the condition that needs them.

### Pickers, command palettes, and permissions

- Anchor a popup to its invoking control when space allows.
- Render one input surface for the focused interaction. If a prompt routes typing through the global composer, use the prompt for context and actions without drawing a second field.
- Use one clear selected row, a persistent query, and visible empty/no-match behavior.
- Keep selection visible while filtering and resizing.
- Put the safest or most common choice in a predictable position; do not rely on color to signal risk.
- Make `Esc` dismissal and focus restoration consistent.

### Status and footer

- Show only state that changes the user's next action.
- Prefer one calm line to several badges or counters.
- Keep predictive context capacity visible by default between model/reasoning and queue count when it is known; hide it before primary state under width pressure.
- Remove labels when position and value already explain the field.
- Hide low-priority metadata before shortening primary content.

## Motion and live state

Motion should explain activity or continuity. Use a single restrained spinner or phase marker for active work. Keep its cell width fixed, stop it immediately on completion or input, and avoid animating multiple regions.

Streaming text should append without reflowing stable content. Coalesce status churn. Reserve enough width for values that change frequently, or update them in place. Popup entry and dismissal should not leave stale cells or cause the transcript to jump.

## Responsive behavior

At each content-pressure threshold:

1. keep the active choice, draft, current output, and recovery text;
2. shorten labels and paths around their meaningful segment;
3. hide secondary metadata and hints;
4. wrap primary content by display width;
5. scroll bounded collections while keeping focus visible;
6. remove decorative spacing and chrome.

Test immediately below and above every changed breakpoint. Tiny terminals still need one usable composer row or a direct explanation that input is unavailable. Clamp all rectangles, offsets, selections, and cursor positions.

## State matrix

Select the rows touched by the change:

| Area       | States                                                                                |
| ---------- | ------------------------------------------------------------------------------------- |
| Composer   | empty, drafting, multiline, command, queued, disabled, validation error, large paste  |
| Agent      | starting, thinking, streaming, waiting, steering, cancelling, complete, failed        |
| Tool       | pending, running, output, truncated, success, failure, cancelled, permission required |
| Picker     | initial, filtered, selected, scrolled, no matches, long value, dismissed              |
| Session    | first run, restored, disconnected provider, recovery required, compacted context      |
| Transcript | short, overflowing, following tail, user-scrolled, resized while streaming            |

Review transitions as carefully as static states: focus, selection, scroll position, draft retention, cursor position, and content movement.

## Polish rubric

Score each dimension from 0 to 2 before calling a redesign complete:

| Dimension   | 0                        | 1                          | 2                                           |
| ----------- | ------------------------ | -------------------------- | ------------------------------------------- |
| Hierarchy   | competing focal points   | readable with effort       | next action is immediate                    |
| Composition | disconnected regions     | mostly balanced            | frame reads as one system                   |
| Rhythm      | arbitrary gaps/insets    | minor inconsistencies      | deliberate spacing and anchors              |
| Restraint   | repeated chrome/noise    | some redundant decoration  | every visual element has a job              |
| Legibility  | color or width dependent | usable in common setup     | clear across target capabilities            |
| State craft | happy path only          | major alternatives covered | transitions and edge states feel resolved   |
| Stability   | jumps or stale cells     | small disturbances         | updates preserve spatial context            |
| Voice       | internal or generic copy | understandable             | concise product language with clear actions |

A zero blocks completion. Use the score to locate the weak dimension, then make a concrete correction. Do not optimize for a perfect total by adding decoration.

## Reference harnesses

Adopt patterns, not screenshots.

Codex demonstrates a restrained terminal-native palette, open transcript, explicit composer states, bounded redraws, and careful terminal restoration. Study its [style guide](https://github.com/openai/codex/blob/main/codex-rs/tui/styles.md), [composer](https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/chat_composer.rs), and [terminal owner](https://github.com/openai/codex/blob/main/codex-rs/tui/src/tui.rs).

Grok Build demonstrates centralized semantic themes, color-capability quantization, an inline-terminal layer, input normalization, and action/effect separation. Study its [pager application](https://github.com/xai-org/grok-build/tree/main/crates/codegen/xai-grok-pager), [renderer](https://github.com/xai-org/grok-build/tree/main/crates/codegen/xai-grok-pager-render), and [inline Ratatui terminal](https://github.com/xai-org/grok-build/tree/main/crates/codegen/xai-ratatui-inline).

Amp demonstrates progressive disclosure through a command palette, external-editor escape hatch, collapsible detail, and distinct queue/steer/interrupt actions. Use the [Amp Owner's Manual](https://ampcode.com/manual) as the current behavior reference.

Factory Droid demonstrates explicit modes, a command palette, shell-mode feedback, review/approval actions, and compact status information. Use the [Factory CLI reference](https://docs.factory.ai/reference/cli-reference) rather than copied screenshots.

The example skills remain useful catalogs: [Hyperb1iss TUI design](https://github.com/hyperb1iss/hyperskills/tree/main/skills/tui-design) covers responsive layouts, focus, help tiers, and accessibility; [Pageton TUI design](https://github.com/pageton/tui-design-skill) provides a practical design/redesign/review workflow. This skill narrows both to thndrs and its current renderer.
