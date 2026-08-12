# UI Polish

This milestone makes the TUI feel calm, coherent, and pleasant without adding
new product capabilities. The transcript remains the primary surface, the
composer remains the action surface, and metadata earns space only when it
helps the user decide what to do next.

The renderer already has the right boundaries: application state projects into
semantic views, then rows, then the Ratatui frame. It also has stable transcript
updates, grouped activity, a bottom-pinned composer, responsive truncation,
mouse selection, and focused detail surfaces. This work should refine those
parts rather than introduce another rendering path or redesign their behavior.

## Composition

Ordinary prose shares a readable measure, capped within roughly 100–120
columns on wide terminals. Transcript copy, startup content, and composer text
use the same left rail. Diffs, code, tables, logs, and focused details may use
the remaining width when it improves comprehension.

The interface has three density bands:

- Comfortable at 120 columns and above, with the full useful hierarchy.
- Compact from 80 to 119 columns, with tighter gaps and shorter metadata.
- Cramped below 80 columns, where low-value labels disappear before content or
  controls do.

Spacing follows a small vocabulary: related lines are adjacent, conversation
turns have one clear break, and major regions receive at most one additional
blank row. Alignment and whitespace provide most of the structure; borders and
filled backgrounds remain reserved for focus or containment.

## Conversation and action

Startup should identify thndrs, surface material warnings, and offer one short
orientation line. Routine capability inventories, paths, and shortcuts stay
hidden unless they affect the current run or the user asks for them.

User turns are clear anchors without becoming heavy cards. Assistant prose is
the quiet default. System and operational entries appear only when they change
what the user should understand or do. Routine successful activity settles
into compact semantic summaries; active work and failures remain prominent,
and detail views retain complete technical evidence.

The composer reads as one bottom action area through a shared rail, proximity,
and a state accent. Only editable input rows use the filled input surface;
session, status, and footer metadata stay on the terminal background. One
authoritative status near the composer communicates ready, sending, streaming,
tool, stopped, and failed states. Existing cursor movement, multiline editing,
history, commands, mentions, queues, scrolling, selection, and cancellation
must not change.

## Visual language

Themes expose semantic roles rather than letting components choose arbitrary
palette colors: primary text, secondary text, accent, active, success, warning,
failure, selection, input surface, and focused surface. Most settled content
uses primary or secondary text. The accent marks focus and identity; status
colors communicate state rather than decorate containers.

This follows two useful Rust harness precedents:

- [Grok Build's pager configuration](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/05-configuration.md)
  adapts between full, minimal, and compact presentation, groups repeated tool
  work, and collapses settled edit detail. thndrs should adopt the density and
  progressive-disclosure principles, not add more user-facing modes.
- [Codex CLI](https://learn.chatgpt.com/docs/codex/cli) keeps the conversation,
  composer, and compact footer in a stable hierarchy. Its
  [TUI style guide](https://github.com/openai/codex/blob/main/codex-rs/tui/styles.md)
  favors terminal-default primary text, dim secondary text, and a small set of
  semantic status colors. thndrs should preserve its themes while applying the
  same restraint through named roles.

## Acceptance

- The current action is obvious in startup, idle, streaming, tool, stopped,
  and failure states without competing status messages.
- Prose, prompts, and chrome share stable rails and a readable measure; rich
  technical output can expand when useful.
- Startup, transcript roles, activity summaries, composer, status, and footer
  read as parts of one interface at comfortable, compact, and cramped widths.
- Settled routine work is quieter than active work, and errors remain easy to
  find and inspect.
- Color improves state recognition but is never the only state cue.
- No existing keyboard, mouse, scrolling, editing, queue, or terminal lifecycle
  behavior regresses.
- Full-frame snapshots cover representative states at 80×24, 120×32, 160×40,
  and one cramped width.

## Boundaries and verification

This milestone does not add a sidebar, tabs, command palette, animation,
inertial scrolling, new tools, new persistence, a renderer rewrite, or new
dependencies. Keep layout, projection, and style decisions pure where
practical, and extend the existing snapshot and terminal harnesses.
