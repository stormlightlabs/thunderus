# iocraft Reference

Reference for iocraft's component model, layout system, and hooks.
Based on iocraft's own APIs and docs. ([GitHub][1], [Docs.rs][2])

## Mental Model

### Everything is a component

iocraft is declarative and retained-mode. You define a tree of components via the
`element!` macro. iocraft diffs state, computes layout via Taffy, and renders to
the terminal. Components re-render when their state changes -- not on a timer.

### Layout is flexbox

All layout is computed by [Taffy][3], a high-performance Rust flexbox/grid engine.
Every `View` maps to a Taffy node. You express layout intent with CSS flexbox
properties (`flex_direction`, `justify_content`, `align_items`, `flex_grow`, `gap`,
`padding`, `margin`, `width`, `height`, etc.).

### State lives in hooks

Components are stateless functions. State, side effects, async work, and event
handling are added via hooks (`use_state`, `use_future`, `use_effect`,
`use_terminal_events`). Hooks must be called in the same order every render --
no conditionals or loops around hook calls.

## Core APIs

### `element!` macro

Declares a component tree in JSX/SwiftUI-like syntax:

```rust
element! {
    View(flex_direction: FlexDirection::Column, padding: 1) {
        Text(content: "Hello", color: Color::Blue)
        View(border_style: BorderStyle::Round, border_color: Color::Cyan) {
            Text(content: "Boxed content")
        }
    }
}
```

- Component names are PascalCase
- Props are `name: value` pairs in parentheses
- Children go in braces `{}`
- Use `#(iter.map(|x| element! { ... }))` for dynamic lists

### `#[component]` macro

Defines a custom component:

```rust
#[component]
fn MyComponent(props: &MyProps, hooks: &mut Hooks) -> impl Into<AnyElement<'static>> {
    let mut count = hooks.use_state(|| 0);
    element! {
        Text(content: format!("Count: {}", count))
    }
}
```

- Takes optional `props: &MyProps` and/or `hooks: &mut Hooks`
- Must return `impl Into<AnyElement<'static>>`

### `#[derive(Default, Props)]`

Makes a struct usable as component props:

```rust
#[derive(Default, Props)]
struct MyProps {
    label: String,
    count: i32,
    selected: bool,
}
```

- Must also derive `Default`
- Props are passed by reference (no cloning)
- Generic type parameters must be `'static`

## Built-in Components

### `View`

The fundamental container. Equivalent to `<div>` in HTML or `Block` in ratatui.

**Layout props** (all optional, map to Taffy `Style` fields):

| Prop                            | Type                  | Default     | Description                                    |
| ------------------------------- | --------------------- | ----------- | ---------------------------------------------- |
| `flex_direction`                | `FlexDirection`       | `Row`       | `Row`, `Column`, `RowReverse`, `ColumnReverse` |
| `flex_wrap`                     | `FlexWrap`            | `NoWrap`    | `NoWrap`, `Wrap`, `WrapReverse`                |
| `flex_grow`                     | `f32`                 | `0.0`       | How much to grow to fill available space       |
| `flex_shrink`                   | `f32`                 | `1.0`       | How much to shrink when overflowing            |
| `flex_basis`                    | `Auto/Length/Percent` | `Auto`      | Initial main-axis size                         |
| `justify_content`               | `JustifyContent`      | `FlexStart` | Main-axis child alignment                      |
| `align_items`                   | `AlignItems`          | `Stretch`   | Cross-axis child alignment                     |
| `align_content`                 | `AlignContent`        | `Stretch`   | Multi-line cross-axis alignment                |
| `gap`                           | `u16`                 | `0`         | Space between children                         |
| `width`                         | `u16` or `pct`        | auto        | Fixed or percentage width                      |
| `height`                        | `u16` or `pct`        | auto        | Fixed or percentage height                     |
| `min_width`                     | `u16`                 | `0`         | Minimum width                                  |
| `max_width`                     | `u16`                 | unbounded   | Maximum width                                  |
| `padding`                       | `u16`                 | `0`         | Inner spacing (all sides)                      |
| `padding_top/right/bottom/left` | `u16`                 | `0`         | Per-side inner spacing                         |
| `margin`                        | `u16`                 | `0`         | Outer spacing (all sides)                      |
| `margin_top/right/bottom/left`  | `u16`                 | `0`         | Per-side outer spacing                         |

**Visual props**:

| Prop               | Type          | Description                         |
| ------------------ | ------------- | ----------------------------------- |
| `border_style`     | `BorderStyle` | `Round`, `Single`, `Double`, `Bold` |
| `border_color`     | `Color`       | Border foreground color             |
| `background_color` | `Color`       | Fill color                          |

Percentage values use the `pct` suffix: `width: 30 pct`.

### `Text`

Renders styled text content.

| Prop              | Type             | Description         |
| ----------------- | ---------------- | ------------------- |
| `content`         | `String`         | The text to display |
| `color`           | `Color`          | Foreground color    |
| `weight`          | `Weight`         | Bold, normal        |
| `text_decoration` | `TextDecoration` | Underline, etc.     |
| `text_align`      | `TextAlign`      | Left, Center, Right |
| `wrap`            | `TextWrap`       | Wrap, NoWrap        |

### `TextInput`

Interactive text input field.

| Prop        | Type              | Description              |
| ----------- | ----------------- | ------------------------ |
| `has_focus` | `bool`            | Whether input is active  |
| `value`     | `String`          | Current text value       |
| `on_change` | `Handler<String>` | Called when text changes |
| `multiline` | `bool`            | Allow multi-line input   |

### `Button`

Clickable button element.

| Prop      | Type          | Description                 |
| --------- | ------------- | --------------------------- |
| `handler` | `Handler<()>` | Called on Enter/Space/click |

### `MixedText`

Text with mixed styles in a single line.

### `Fragment`

Groups elements without affecting layout (like React Fragment).

### `ContextProvider`

Passes context to all descendants:

```rust
element! {
    ContextProvider(value: my_theme) {
        MyComponent
    }
}
```

## Hooks

All hooks are called on the `hooks: &mut Hooks` parameter.

### `use_state`

Reactive state. Mutations trigger re-render.

```rust
let mut count = hooks.use_state(|| 0);
count += 1;         // triggers re-render
let val = *count;   // read current value
count.set(42);      // explicit set
```

### `use_ref`

Mutable value that does NOT trigger re-render.

```rust
let scroll_offset = hooks.use_ref(|| 0usize);
```

### `use_const`

Immutable value stored for the component's lifetime.

### `use_context`

Read context provided by an ancestor `ContextProvider`:

```rust
let theme = hooks.use_context::<Theme>();
```

### `use_effect`

Run side effects after render when dependencies change:

```rust
hooks.use_effect(|| {
    // runs after render
}, &[dep1, dep2]);
```

### `use_memo`

Memoize expensive computation, recompute only when deps change.

### `use_future`

Spawn an async task bound to the component's lifetime:

```rust
hooks.use_future(async move {
    loop {
        smol::Timer::after(Duration::from_millis(100)).await;
        count += 1;
    }
});
```

### `use_async_handler`

Create a `Handler` that runs async work.

### `use_terminal_events`

Listen for keyboard and mouse events:

```rust
hooks.use_terminal_events(|event| match event {
    TerminalEvent::Key(KeyEvent { code: KeyCode::Char('q'), .. }) => {
        // handle quit
    }
    _ => {}
});
```

### `use_terminal_size`

Returns current terminal dimensions `(width, height)`.

### `use_output`

Write to stdout/stderr from a component (output appears above the rendered UI).

## Rendering Modes

### One-shot: `.print()`

Render once to stdout and return. For static output (tables, reports, CLI help).

```rust
fn main() {
    element! {
        View(border_style: BorderStyle::Round) {
            Text(content: "Hello, world!")
        }
    }
    .print();
}
```

### Interactive: `.render_loop()`

Fullscreen interactive mode. Takes over the terminal, re-renders on state change.
Returns a future -- must be awaited.

```rust
fn main() {
    smol::block_on(element!(App).render_loop()).unwrap();
}
```

iocraft handles raw mode, alternate screen, and cleanup automatically.

## Taffy Layout Engine

[Taffy][3] is the flexbox engine behind iocraft's layout system.

### How it works

1. Each `View`/component maps to a node in a `TaffyTree`.
2. Each node has a `Style` struct with CSS flexbox properties.
3. `compute_layout()` runs the flexbox algorithm on the tree.
4. Each node gets a `Layout` with computed `(x, y, width, height)`.
5. iocraft renders each component at its computed position.

### Key Taffy `Style` fields

These are the CSS flexbox properties that iocraft exposes via `View` props:

| Property                | Values                                                             | Default   | Behavior                                     |
| ----------------------- | ------------------------------------------------------------------ | --------- | -------------------------------------------- |
| `display`               | Flex, Grid, Block, None                                            | Flex      | Layout algorithm for children                |
| `flex_direction`        | Row, Column, RowReverse, ColumnReverse                             | Row       | Main axis direction                          |
| `flex_wrap`             | NoWrap, Wrap, WrapReverse                                          | NoWrap    | Whether children wrap                        |
| `flex_grow`             | f32                                                                | 0.0       | Proportion of extra space absorbed           |
| `flex_shrink`           | f32                                                                | 1.0       | Proportion of overflow absorbed              |
| `justify_content`       | FlexStart, FlexEnd, Center, SpaceBetween, SpaceAround, SpaceEvenly | FlexStart | Main-axis alignment                          |
| `align_items`           | FlexStart, FlexEnd, Center, Stretch, Baseline                      | Stretch   | Cross-axis alignment                         |
| `align_content`         | FlexStart, FlexEnd, Center, Stretch, SpaceBetween, SpaceAround     | Stretch   | Multi-line cross-axis                        |
| `gap`                   | Length                                                             | 0         | Space between children (row_gap, column_gap) |
| `size`                  | Auto, Length, Percent                                              | Auto      | Explicit width/height                        |
| `min_size` / `max_size` | Auto, Length, Percent                                              | 0 / none  | Size bounds                                  |
| `padding`               | Length (per-side)                                                  | 0         | Inner spacing                                |
| `margin`                | LengthPercentageAuto (per-side)                                    | 0         | Outer spacing                                |
| `position`              | Relative, Absolute                                                 | Relative  | Positioning mode                             |
| `inset`                 | LengthPercentageAuto (per-side)                                    | Auto      | Offsets for absolute positioning             |

### Measure functions

Leaf nodes with intrinsic sizes (text) provide a measure callback. During layout,
Taffy calls this function with available space constraints and the node returns its
natural size. iocraft handles this for `Text` components automatically.

### Flexbox mental model

```text
Main axis (flex_direction)
[=====|=====|=====]     <- justify_content controls spacing along this axis
  ^       ^       ^
  |   align_items controls alignment along cross axis
  |
  flex_grow/flex_shrink controls how items share space
```

- `flex_grow: 0.0` = item keeps its natural size
- `flex_grow: 1.0` = item expands to fill remaining space
- Two items with `flex_grow: 1.0` each = equal split
- `flex_grow: 2.0` vs `flex_grow: 1.0` = 2:1 ratio

## Patterns

### App shell: header / body / footer

The most common TUI layout is a fixed-height header and footer with a body that
fills all remaining vertical space. Set the outer `View` to `FlexDirection::Column`
so children stack vertically. Give the header and footer explicit `height` values
and the body `flex_grow: 1.0` -- flexbox will assign it whatever rows are left over.
This is robust on resize because the body absorbs all size changes.

```rust
element! {
    View(flex_direction: FlexDirection::Column, flex_grow: 1.0) {
        // Header (fixed height)
        View(height: 3, border_style: BorderStyle::Single) {
            Text(content: "Thunderus")
        }
        // Body (fills remaining space)
        View(flex_grow: 1.0) {
            // content here
        }
        // Footer (fixed height)
        View(height: 1) {
            Text(content: "Press ? for help", color: Color::DarkGrey)
        }
    }
}
```

### Sidebar + main content

A horizontal split where the sidebar has a fixed column count and the main content
stretches to fill. Use `FlexDirection::Row` on the parent and give the sidebar a
fixed `width`. The main pane gets `flex_grow: 1.0` to consume the remainder. To
make the sidebar responsive but bounded, replace the fixed width with `min_width`
and `max_width` constraints.

```rust
element! {
    View(flex_direction: FlexDirection::Row, flex_grow: 1.0) {
        // Sidebar (fixed width)
        View(width: 28, border_style: BorderStyle::Single) {
            // nav items
        }
        // Main content (fills rest)
        View(flex_grow: 1.0) {
            // content
        }
    }
}
```

### Centered modal

To center a fixed-size element both vertically and horizontally, use
`justify_content: JustifyContent::Center` (main axis) and
`align_items: AlignItems::Center` (cross axis) on a parent `View` that fills the
screen via `flex_grow: 1.0`. The child specifies explicit `width` and `height`.
This is significantly simpler than the ratatui pattern of splitting vertically
then horizontally with `Fill` constraints on each side.

```rust
element! {
    View(
        flex_grow: 1.0,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
    ) {
        View(
            width: 50,
            height: 12,
            border_style: BorderStyle::Round,
            border_color: Color::Blue,
        ) {
            Text(content: "Modal content")
        }
    }
}
```

### Dynamic list

Render a variable-length collection by mapping an iterator inside the `element!`
macro using the `#(...)` syntax. Each item produces its own element subtree.
The parent `View` with `FlexDirection::Column` stacks them vertically. This
replaces the ratatui pattern of pre-computing heights, slicing visible items,
and rendering each into a manually-offset `Rect`.

```rust
element! {
    View(flex_direction: FlexDirection::Column) {
        #(items.iter().map(|item| element! {
            View(padding_left: 1) {
                Text(content: item.label.clone())
            }
        }))
    }
}
```

### Card with selection indicator

A reusable card component that changes its border color based on selection state.
Define a `Props` struct with the data the card needs, then use the `#[component]`
macro to create a function that reads props and returns an element tree. The
border color is computed from `props.selected` before building the element --
all conditional logic happens in Rust, not in the template syntax.

```rust
#[derive(Default, Props)]
struct CardProps {
    label: String,
    selected: bool,
}

#[component]
fn Card(props: &CardProps) -> impl Into<AnyElement<'static>> {
    let border_color = if props.selected { Color::Cyan } else { Color::DarkGrey };
    element! {
        View(
            border_style: BorderStyle::Round,
            border_color: border_color,
            padding: 1,
        ) {
            Text(content: props.label.clone())
        }
    }
}
```

### Theme via context

Instead of importing a global `colors` module, define a `Theme` struct and pass it
down the component tree via `ContextProvider`. Any descendant component can read the
theme with `hooks.use_context::<Theme>()`. This makes theming composable -- you can
swap the theme at any level of the tree, and all children beneath that provider will
pick up the new values without prop drilling.

```rust
struct Theme {
    accent: Color,
    bg: Color,
    text: Color,
    muted: Color,
    border: Color,
}

#[component]
fn App(hooks: &mut Hooks) -> impl Into<AnyElement<'static>> {
    let theme = Theme {
        accent: Color::Cyan,
        bg: Color::Black,
        text: Color::White,
        muted: Color::DarkGrey,
        border: Color::DarkGrey,
    };

    element! {
        ContextProvider(value: theme) {
            MainContent
        }
    }
}

#[component]
fn MainContent(hooks: &mut Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_context::<Theme>();
    element! {
        View(background_color: theme.bg) {
            Text(content: "Themed text", color: theme.text)
        }
    }
}
```

### Interactive input with keyboard handling

A minimal chat-style interface demonstrating the full iocraft input cycle. The
`use_state` hook holds both the input buffer and the message history.
`use_terminal_events` receives keyboard events -- Enter submits the current input
and appends it to the message list, Ctrl+Q quits. The `TextInput` component handles
character-level editing internally; the parent only needs to provide `value`,
`on_change`, and `has_focus`. The message list re-renders automatically when state
changes because `use_state` mutations trigger a new render pass.

```rust
#[component]
fn InputScreen(hooks: &mut Hooks) -> impl Into<AnyElement<'static>> {
    let mut input = hooks.use_state(|| String::new());
    let mut messages = hooks.use_state(|| Vec::<String>::new());

    hooks.use_terminal_events({
        let input = input.clone();
        let messages = messages.clone();
        move |event| match event {
            TerminalEvent::Key(KeyEvent { code: KeyCode::Enter, .. }) => {
                let text = input.to_string();
                if !text.is_empty() {
                    messages.write(|m| m.push(text));
                    input.set(String::new());
                }
            }
            TerminalEvent::Key(KeyEvent { code: KeyCode::Char('q'), modifiers, .. })
                if modifiers.contains(KeyModifiers::CONTROL) => {
                // quit
            }
            _ => {}
        }
    });

    element! {
        View(flex_direction: FlexDirection::Column, flex_grow: 1.0) {
            // Messages
            View(flex_grow: 1.0, flex_direction: FlexDirection::Column) {
                #(messages.read().iter().map(|m| element! {
                    Text(content: m.clone())
                }))
            }
            // Input
            View(border_style: BorderStyle::Single) {
                TextInput(has_focus: true, value: input.to_string(), on_change: move |v| input.set(v))
            }
        }
    }
}
```

## Rules of Thumb

1. **Use `View` with `flex_direction: FlexDirection::Column` for vertical stacking.**
   This replaces `Layout::vertical([...])`.

2. **Use `flex_grow: 1.0` for "fill remaining space".**
   This replaces `Constraint::Min(0)` and `Constraint::Fill(1)`.

3. **Use fixed `width`/`height` for chrome, `flex_grow` for content.**
   Same principle as ratatui's Length for chrome, Min/Fill for content.

4. **Use `gap` instead of manual spacers.**
   Replaces ratatui's `spacing()`.

5. **Use `padding` on `View` instead of margin hacks.**
   Maps to ratatui's `margin()` on layouts and `Padding` on blocks.

6. **Provide theme colors via context, not global constants.**
   `ContextProvider` + `use_context` replaces importing a `colors` module.

7. **One component per concern.**
   iocraft components are cheap. Split aggressively.

8. **Keep hooks at the top of the component function.**
   Never call hooks inside `if`, `match`, `for`, or closures.

9. **Use `use_future` for async polling, not manual event loops.**
   Channel receivers go in `use_future`, state updates trigger re-renders.

10. **Use `use_terminal_events` for keyboard input.**
    Replaces the `crossterm::event::read()` + dispatch pattern.

## Gotchas

### `Box` was renamed to `View`

Older examples (pre-0.5) use `Box`. The current API uses `View`.

### smol, not tokio

iocraft's async runtime is smol. `use_future` spawns on smol. If your project uses
tokio elsewhere, keep the runtimes on separate threads and communicate via
`std::sync::mpsc` or `flume` channels.

### Props must be covariant

The `#[derive(Props)]` macro enforces covariance. You cannot store mutable references
or certain lifetimed types in props structs.

### No built-in scroll container

You must implement virtual scrolling yourself: track offset in `use_state`, slice
the visible range, render only what fits.

### Flexbox only

No CSS Grid in the current iocraft API (though Taffy supports it). No absolute
positioning beyond what flexbox provides.

### Hook ordering

Like React, hooks must be called in the same order every render. Calling a hook
inside a conditional or loop is undefined behavior.

### Fullscreen takes over the terminal

`render_loop()` enters alternate screen mode. You lose access to terminal scrollback.
For chat UIs that need scroll history, implement a virtual viewport.

## References

- iocraft GitHub ([GitHub][1])
- iocraft API docs ([Docs.rs][2])
- Taffy layout engine ([GitHub][3])
- Taffy API docs ([Docs.rs][4])
- Taffy `Style` struct ([Docs.rs][5])
- iocraft examples ([GitHub][6])
- iocraft `View` component ([Docs.rs][7])
- iocraft hooks ([Docs.rs][8])

[1]: https://github.com/ccbrown/iocraft "iocraft GitHub"
[2]: https://docs.rs/iocraft/latest/iocraft/ "iocraft Docs.rs"
[3]: https://github.com/DioxusLabs/taffy "Taffy GitHub"
[4]: https://docs.rs/taffy/latest/taffy/ "Taffy Docs.rs"
[5]: https://docs.rs/taffy/latest/taffy/struct.Style.html "Taffy Style"
[6]: https://github.com/ccbrown/iocraft/tree/main/examples "iocraft examples"
[7]: https://docs.rs/iocraft/latest/iocraft/components/struct.View.html "View"
[8]: https://docs.rs/iocraft/latest/iocraft/hooks/index.html "Hooks"
