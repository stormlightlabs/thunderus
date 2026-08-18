# Ratatui and terminal engineering

Use this reference before changing renderer boundaries, terminal lifecycle, input handling, colors, animation, Unicode layout, or dependencies.

## Contents

- Project baseline
- Architecture
- Terminal lifecycle
- Rendering and scheduling
- Text, Unicode, and ANSI
- Color and themes
- Dependency decisions
- Primary references

## Project baseline

The workspace currently uses Ratatui 0.30 with the Crossterm backend and Crossterm 0.29. It already depends on `unicode-segmentation`, `unicode-width`, `insta`, and `vt100`.

Ratatui is an immediate-mode renderer. Each draw builds the complete current buffer; Ratatui diffs it against the previous buffer before writing cells. Rendering order matters because later widgets overwrite earlier cells. A render function should therefore be a deterministic projection of current state, not a place to mutate the application or launch work.

Avoid two incompatible Crossterm versions in the dependency graph. Ratatui's backend documentation warns that they can maintain separate event queues and raw-mode state, lose events, and fail to restore the terminal correctly.

## Architecture

Keep these boundaries:

```text
effects and provider events
        |
        v
application state --input/action--> state transition
        |
        v
RendererView --pure projection--> surfaces/rows --layout--> frame buffer
        |
        v
terminal owner --diff/write--> Crossterm backend
```

- Application state owns domain and interaction state.
- `RendererView` exposes only what rendering needs.
- Surface builders choose semantic roles and bounded content.
- Layout helpers own cell measurement, wrapping, truncation, and rectangle math.
- The terminal owner handles modes, event polling/streaming, cursor visibility, and restoration.

Use Ratatui widgets when their buffer behavior matches the surface. Keep the existing row projection when it provides more deterministic transcript, inline, or ANSI output. Do not mix direct terminal writes into a Ratatui frame except through the terminal owner or an intentionally synchronized extension.

## Terminal lifecycle

Treat terminal modes as paired resources:

- raw mode: enable / disable;
- alternate screen: enter / leave;
- bracketed paste: enable / disable;
- mouse capture: enable / disable;
- focus events: enable / disable;
- keyboard enhancement flags: push / pop or reset;
- cursor style and visibility: set / restore.

Use a guard whose `Drop` path makes a best effort to restore every enabled mode. On normal exit, return the first restoration error after attempting the remaining cleanup. Also install panic/error handling so an unexpected failure does not leave the shell in raw mode or with a hidden cursor.

Request optional protocols defensively. Kitty keyboard enhancements can disambiguate modified Enter and report press/repeat/release events, but unsupported terminals must retain a usable fallback. Handle `KeyEventKind` deliberately. Most commands should act on press and repeat, not release.

Crossterm requires one event consumption strategy. Use `poll` plus `read` on one thread, or `EventStream`; do not combine the APIs across threads. Coalesce resize bursts before expensive projection where the current event loop permits it.

Bracketed paste must produce paste semantics and be disabled on exit. Preserve newlines according to composer policy. Avoid logging clipboard or paste contents.

## Rendering and scheduling

- Draw a complete logical frame from current state.
- Trigger redraws for state changes, input, resize, and bounded animation ticks.
- Coalesce redundant requests while streaming.
- Keep filesystem, network, process, and provider work out of the render path.
- Avoid cloning an entire transcript on every frame when a stable projection or cached viewport can be invalidated precisely.
- Use synchronized updates only when they solve measured tearing in supported terminals and retain a fallback.
- Keep the cursor hidden during frame updates if the backend requires it, then restore its intended position and visibility.

Ratatui's buffer diff already limits cell writes. Do not add dirty rectangles until profiling proves that state projection or buffer construction, rather than terminal I/O, is the problem.

For a normal-screen inline viewport, a terminal resize can reflow old native-history cells at character boundaries and can move wrapped live rows outside their remembered rectangle. Rebuild app-owned transcript rows at the new width after purging the app's visible/history surface; clear only the mutable pane for height-only changes, and keep prose wrapping word-aware before painting.

## Text, Unicode, and ANSI

Terminal layout uses display cells:

- segment editable text by grapheme cluster when cursor movement must preserve a visible character;
- measure output with `unicode-width` or the project's established display-width helpers;
- never index rendered columns by UTF-8 byte offset;
- clamp cursor and selection positions after truncation, wrapping, resize, and deletion;
- test emoji ZWJ sequences, regional indicators, combining marks, CJK, zero-width characters, tabs, and long words.

Ratatui stores text and style separately in cells. Raw ANSI escape sequences inside a `Span` do not become style. Strip untrusted control sequences or parse trusted styled output into Ratatui text. Never pass provider or tool output through to the terminal as executable escape sequences.

## Color and themes

Prefer semantic fields such as `text_primary`, `text_muted`, `focus`, `success`, `warning`, and `error`. Components should not name a theme's decorative swatches.

Two viable policies exist:

1. Terminal-native: use `Color::Reset` for ordinary foreground/background and a small ANSI semantic palette. This inherits user contrast choices and is the safest baseline.
2. Managed theme: define colors centrally, detect color capability, quantize RGB to 256/16-color output, and provide a terminal-native/no-color fallback.

Honor `NO_COLOR`. Do not infer truecolor from one environment variable alone. Theme detection is imperfect, especially under tmux, SSH, Windows consoles, and multiplexers; make failure degrade to legible defaults.

If supporting light and dark themes, test actual cell combinations for primary text, muted text, selection, focused input, diffs, and errors. Never paint a foreground and assume the terminal's unknown background will contrast.

## Dependency decisions

The presence of a crate is not a reason to add it. Start with the installed stack.

| Need                       | Candidate                                                                                    | Add only when                                                                                                                                                                                       |
| -------------------------- | -------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Multiline editor           | [`tui-textarea-2`](https://docs.rs/tui-textarea-2/latest/tui_textarea/)                      | Its editing, selection, history, wrapping, and atomic-range model replaces more code than integration would duplicate. Migrating the current composer is a product change and needs explicit scope. |
| Scrollable compound widget | [`tui-scrollview`](https://docs.rs/tui-scrollview/latest/tui_scrollview/)                    | A new surface needs two-dimensional/stateful scrolling that current viewport helpers cannot express cleanly. Do not replace transcript scrolling merely for API uniformity.                         |
| ANSI to Ratatui text       | [`ansi-to-tui`](https://docs.rs/ansi-to-tui/latest/ansi_to_tui/)                             | Trusted styled subprocess output must retain ANSI styling. Sanitize control sequences and hyperlinks separately.                                                                                    |
| Syntax highlighting        | [`syntect`](https://docs.rs/syntect/latest/syntect/)                                         | Code-fence fidelity is a requested feature and theme mapping, language fallback, caching, and large-file costs are specified.                                                                       |
| Terminal color detection   | [`supports-color`](https://docs.rs/supports-color/latest/supports_color/)                    | Existing detection cannot cover a concrete compatibility case. Pair capability detection with `NO_COLOR` and semantic fallbacks.                                                                    |
| Background appearance      | [`terminal-light`](https://docs.rs/terminal-light/latest/terminal_light/) or an OSC 11 probe | Automatic light/dark behavior is requested and timeout, tmux/SSH, unsupported-terminal, and user-override paths are designed.                                                                       |
| Images                     | [`ratatui-image`](https://docs.rs/ratatui-image/latest/ratatui_image/)                       | Inline image display is a product requirement with protocol fallback, resize, memory, remote-session, and text-only behavior.                                                                       |
| Visual effects             | [`tachyonfx`](https://docs.rs/tachyonfx/latest/tachyonfx/)                                   | A specific transition improves orientation, stops immediately on input, respects reduced/no-motion behavior, and does not require a permanent render loop.                                          |
| Clipboard images/text      | [`arboard`](https://docs.rs/arboard/latest/arboard/)                                         | Native clipboard behavior is requested and platform availability, sandboxing, privacy, and non-GUI fallback are handled.                                                                            |
| Panic reports              | [`color-eyre`](https://docs.rs/color-eyre/latest/color_eyre/)                                | It integrates with the terminal restoration guard and improves actionable diagnostics without exposing secrets.                                                                                     |

Check the candidate's Ratatui and Crossterm ranges before adding it. A second incompatible Crossterm version is a rejection condition. Prefer feature flags that avoid unused backends and protocols.

## Primary references

- [Ratatui rendering model](https://ratatui.rs/concepts/rendering/under-the-hood/)
- [Ratatui application patterns](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/)
- [Ratatui widget composition](https://ratatui.rs/concepts/widgets/)
- [Ratatui backend and Crossterm compatibility](https://ratatui.rs/concepts/backends/)
- [Crossterm event module](https://docs.rs/crossterm/0.29.0/crossterm/event/)
- [Crossterm bracketed paste](https://docs.rs/crossterm/0.29.0/crossterm/event/struct.EnableBracketedPaste.html)
- [Crossterm keyboard enhancement flags](https://docs.rs/crossterm/0.29.0/crossterm/event/struct.KeyboardEnhancementFlags.html)
- [Unicode Standard Annex #29: grapheme boundaries](https://www.unicode.org/reports/tr29/)
