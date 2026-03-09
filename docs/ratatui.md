# Ratatui Layout Cookbook

This is a layout cookbook for Ratatui’s built-in layout system:

- `Rect` + `Layout` + `Constraint` (+ `Flex`, `margin`, `spacing`)

Everything here is based on Ratatui’s own APIs and docs. ([Ratatui][1])

## Mental model

### Everything is a `Rect`

Rendering in Ratatui is immediate mode, i.e. every frame you compute rectangles
and render widgets into them.

You almost always start from `frame.area()` (the terminal-sized `Rect` for the
current draw call). ([Docs.rs][2])

### `Layout` *splits* a `Rect` into more `Rect`s

A `Layout` is:

- **direction**: vertical or horizontal
- **constraints**: how each segment gets sized
- optional **margin**: inset from the outer area
- optional **spacing**: gaps between segments
- optional **flex**: distributes "extra" space (flexbox-ish) ([Docs.rs][3])

### Constraints define allocation rules (and there is an order)

Constraints include `Length`, `Percentage`, `Ratio`, `Fill`, `Min`, `Max`, etc.
Ratatui documents that **relative constraints (`Percentage`, `Ratio`) are computed relative to the entire area being divided** (not "leftover after fixed sizes"), and it documents a prioritization order. ([Docs.rs][4])

## Architecture

### TEA/Model-View-Update

Thunderus UI follows a TEA-style structure:

- **Model**: `App` owns all screen state (`welcome`, `chat`, `files`, `settings`, `help`).
- **Update**: `update` functions handle messages and return commands.
- **View**: each frame is redrawn from state using pure-ish screen `view` functions.

This keeps event handling, state transitions, side-effect dispatch, and rendering
separate, and aligns with Ratatui guidance on application patterns. ([Ratatui][9])

### Chat transcript behavior

The chat viewport auto-follows the latest content, while older output is expected
to be reviewed via the terminal's native scrollback behavior.

## Core APIs

### `Layout` construction

Ratatui provides ergonomic constructors like `Layout::vertical([...])` / `Layout::horizontal([...])`
and `.split(area)` to get sub-rectangles. ([Docs.rs][3])

### `Rect` helpers (ergonomic splitting)

Modern Ratatui also offers `Rect::layout(&Layout)` / `Rect::split(&Layout)`-style
helpers that return fixed-size arrays when the number of constraints is known at compile time, enabling destructuring. ([Docs.rs][5])

## Rules of Thumb

1. **Prefer nested `Layout`s over clever single-pass constraints.**
   You’ll get simpler reasoning and fewer edge-case bugs when resizing.

2. **Use `Length` for "chrome", `Min`/`Fill` for "content".**
   Example: header/footer are `Length(n)`, main body is `Min(0)` or `Fill(1)`.

3. **Use `margin()` for global padding, `spacing()` for gutters.**

   - `margin` pushes everything inward from the parent rect
   - `spacing` inserts gaps *between* segments ([Docs.rs][3])

4. **Treat `Percentage` and `Ratio` as "responsive intent", not pixel-perfect.**
   Combine with `Min`/`Max` when you need guardrails. ([Docs.rs][4])

5. **When something must never disappear, don’t rely on `Percentage` alone.**
   Add a `Min` or a `Length` segment for critical UI elements.

## Patterns

### App Shell: header / body / footer

```rust
use ratatui::layout::{Constraint, Layout};

let area = frame.area();

let [header, body, footer] = area.layout(&Layout::vertical([
    Constraint::Length(3),
    Constraint::Min(0),
    Constraint::Length(2),
]));
```

- Robust on resize
- Body gets whatever remains (`Min(0)` prevents negative sizing assumptions) ([Ratatui][1])

### Responsive Sidebar + Main Content

```rust
use ratatui::layout::{Constraint, Layout};

let area = frame.area();

let [sidebar, main] = area.layout(&Layout::horizontal([
    Constraint::Length(28), // fixed nav width
    Constraint::Min(0),     // rest
]));
```

Variant: make sidebar responsive but bounded:

```rust
let [sidebar, main] = area.layout(&Layout::horizontal([
    Constraint::Max(35),
    Constraint::Min(0),
]));
```

Constraints like `Min`/`Max` are first-class and intended for these guardrails. ([Docs.rs][4])

### Center a Modal / Dialog

Technique:

1. Split vertically to isolate a centered band
2. Split horizontally inside that band

```rust
use ratatui::layout::{Constraint, Layout};

let area = frame.area();

let vertical = Layout::vertical([
    Constraint::Fill(1),
    Constraint::Length(12), // modal height
    Constraint::Fill(1),
]).split(area);

let band = vertical[1];

let horizontal = Layout::horizontal([
    Constraint::Fill(1),
    Constraint::Length(50), // modal width
    Constraint::Fill(1),
]).split(band);

let modal = horizontal[1];
```

If you want "true centering even when extra space exists", consider using `flex` (see next). ([Docs.rs][3])

### Use `Flex` to control leftover space distribution

Ratatui has a `Flex` enum "loosely based on flexbox" with options like `Start`, `Center`, `End`, `SpaceAround`, `SpaceBetween`. ([Ratatui][6])

Typical use:

- **`Flex::Start`**: pack segments at the start (default in newer releases)
- **`Flex::Center`**: center group of segments within the available space
- **`SpaceBetween`**: push first/last to edges, distribute gaps ([Ratatui][6])

(Exact method names vary by version; follow the current `Layout` docs.rs page for the fluent setter.) ([Docs.rs][3])

### Padding and Gutters

```rust
use ratatui::layout::{Constraint, Layout};

let chunks = Layout::vertical([
    Constraint::Length(3),
    Constraint::Min(0),
])
.margin(1)   // padding
.spacing(1)  // gutter between segments
.split(frame.area());
```

- `margin`: consistent outer padding
- `spacing`: clean separation between sections ([Docs.rs][3])

### Border-safe card backgrounds + text wrapping

If you want a "card" where the content background stays **inside** the border:

1. Render the bordered `Block` on the outer `Rect`.
2. Compute `inner = block.inner(area)`.
3. Fill and render content on `inner` using your card background color.

```rust
let outer = Block::default()
    .borders(Borders::ALL)
    .style(Style::default().bg(colors::BG_TERMINAL));
frame.render_widget(outer.clone(), area);

let inner = outer.inner(area);
frame.render_widget(Block::default().style(Style::default().bg(card_bg)), inner);

let content = Paragraph::new(line)
    .style(Style::default().bg(card_bg))
    .wrap(Wrap { trim: true });
frame.render_widget(content, inner);
```

Important: wrapping alone does not make text visible if the row height is fixed too small.
For bordered cards, budget at least:

- `card_height = content_lines + 2` (`+2` is top/bottom border)

So if a label wraps to 2 lines, the card should be at least `Length(4)`.

### Input row with top border + horizontal padding

For a terminal chat input, a clean pattern is:

1. Reserve a fixed-height row for the input area (`Length(3)` works well).
2. Render a `Block` with `Borders::TOP`.
3. Add horizontal padding on the block (`Padding::new(1, 1, 0, 1)`).
4. Render the input paragraph inside `block.inner(area)`.

```rust
let container = Block::default()
    .borders(Borders::TOP)
    .border_style(Style::default().fg(colors::BORDER_COLOR))
    .style(Style::default().bg(colors::BG_TERMINAL))
    .padding(Padding::new(1, 1, 0, 1));
frame.render_widget(container.clone(), area);

let inner = container.inner(area);
frame.render_widget(input_paragraph, inner);
```

This gives you a single top divider and consistent left/right breathing room without changing parent widths.

### Grid Layout for nested splits

Ratatui doesn’t have a dedicated grid primitive; you build one by nesting:

1. Split into rows
2. Split each row into columns

```rust
use ratatui::layout::{Constraint, Layout};

let area = frame.area();
let rows = Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(area);

let top_cols = Layout::horizontal([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)]).split(rows[0]);
let bot_cols = Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(rows[1]);
```

Use `Ratio` when you want stable proportions under resize. ([Docs.rs][4])

## Layout Gotchas

### Percentages don’t behave like CSS

They’re not meant to. Ratatui states relative constraints are computed relative to the **entire** split area. If you expected "percentage of leftover after fixed sizes", you’ll get surprising results. Use nested splits or `Min/Max` to get what you want. ([Docs.rs][4])

### Widgets overlap

Ratatui will happily render into the same `Rect` twice if you tell it to. Layout is purely a geometry calculator; correctness is on you. Rendering is immediate mode. ([Ratatui][7])

### UI jumps on resize

Stabilize with:

- fixed chrome (`Length`)
- bounded regions (`Min`/`Max`)
- consistent padding/gutters (`margin`/`spacing`)
- fewer "one-shot" complex splits; prefer nested, readable blocks ([Docs.rs][3])

## App Structure

### Layout first, widgets second

In each `draw`:

1. `let area = frame.area();` (always use this, not stale resize event sizes) ([Docs.rs][2])
2. Compute all `Rect`s (nested splits)
3. Render widgets using `frame.render_widget(widget, rect)` ([Docs.rs][2])

This keeps layout deterministic and makes testing easier (you can unit test your rect math without rendering).

## References

- Ratatui "Layout" concept doc (overview + examples) ([Ratatui][1])
- `Layout` API on docs.rs (all knobs: direction/constraints/margin/flex/spacing) ([Docs.rs][3])
- `Constraint` API on docs.rs (behavior + prioritization) ([Docs.rs][4])
- `Rect` API on docs.rs (ergonomic layout helpers) ([Docs.rs][5])
- Flex example + notes ([Ratatui][8])
- Ratatui application patterns / TEA ([Ratatui][9])
- The Elm Architecture overview ([Elm][10])

[1]: https://ratatui.rs/concepts/layout/ "Layout"
[2]: https://docs.rs/ratatui/latest/ratatui/struct.Frame.html "Frame in ratatui - Rust"
[3]: https://docs.rs/ratatui/latest/ratatui/layout/struct.Layout.html "Layout in ratatui::layout - Rust"
[4]: https://docs.rs/ratatui/latest/ratatui/layout/enum.Constraint.html "Constraint in ratatui::layout - Rust"
[5]: https://docs.rs/ratatui/latest/ratatui/layout/struct.Rect.html "Rect in ratatui::layout - Rust"
[6]: https://ratatui.rs/highlights/v026/ "v0.26.0"
[7]: https://ratatui.rs/concepts/rendering/ "Rendering"
[8]: https://ratatui.rs/examples/layout/flex/ "Flex"
[9]: https://ratatui.rs/concepts/application-patterns/the-elm-architecture/ "The Elm Architecture"
[10]: https://guide.elm-lang.org/architecture/ "The Elm Architecture"
