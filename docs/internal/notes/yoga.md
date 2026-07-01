---
title: Yoga Layout Engine
Sources:
  - https://www.yogalayout.dev/docs/about-yoga
  - https://www.yogalayout.dev/docs/getting-started/laying-out-a-tree
  - https://www.yogalayout.dev/docs/getting-started/configuring-yoga
  - https://www.yogalayout.dev/docs/advanced/incremental-layout
  - https://www.yogalayout.dev/docs/advanced/external-layout-systems
  - https://www.yogalayout.dev/docs/styling/
  - https://github.com/react/yoga
  - https://docs.rs/crate/yoga/latest
  - https://docs.rs/taffy/latest/taffy/
Author: Meta / Yoga project, yoga-rs maintainers, Taffy maintainers
Date: 2026-07-01
Tags: [layout, yoga, flexbox, ui, rust, terminal-ui]
---

## Summary

Yoga is an embeddable, cross-language layout engine that computes the size and
position of boxes using a CSS/Flexbox-like model; it does not render pixels,
terminal cells, text, or UI widgets by itself.

## Key Ideas

- **Yoga is only a layout engine:** Its job is box geometry. Applications still
  own rendering, input, accessibility, hit testing, text shaping, and drawing.
- **The core model is a tree:** Each box is a Yoga node. Nodes store style input,
  parent/child relationships, and computed layout output.
- **The style system is Flexbox-centered:** Yoga supports a subset of CSS,
  mostly focused on Flexbox: direction, wrapping, alignment, justification,
  margin, padding, border, dimensions, min/max sizes, gaps, positioning,
  display, and overflow.
- **Layout results are numeric rectangles:** After layout, each node exposes a
  position relative to its parent and a computed width/height.
- **Text and other content can be measured externally:** Leaf nodes can use
  measure functions so Yoga can ask another system for natural size under
  constraints.
- **Dirty tracking supports incremental layout:** Style or child changes mark
  affected nodes and ancestors dirty. Later layout can skip clean subtrees when
  constraints are unchanged.
- **Yoga's defaults are not exactly browser defaults:** It defaults to column
  direction, `flex-shrink: 0`, `position: relative`, and other behaviors unless
  configured.
- **Bindings vary by ecosystem:** Yoga is written in C++ with a C API. The Rust
  `yoga` crate binds to it; Taffy is a separate pure-Rust layout engine with a
  similar problem domain.

## Claims & Evidence

| Claim                                                               | Support                                                                                                                                             | Caveat / Confidence |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------- |
| Yoga computes layout but does not render UI.                        | Official docs describe Yoga as responsible for determining box sizes and positions, not drawing.                                                    | High.               |
| Yoga's API is node-tree based.                                      | Getting-started docs show creating nodes, styling them, inserting children, calculating layout, and reading computed results.                       | High.               |
| Yoga is mainly Flexbox-oriented.                                    | Styling docs cover Flexbox properties and note Yoga supports a CSS subset mostly focused on Flexbox.                                                | High.               |
| Yoga uses arbitrary "points" plus percentages, not CSS units.       | Styling docs say Yoga does not operate on `px` or `em`; callers must map their own units into Yoga points.                                          | High.               |
| Text measurement remains outside Yoga.                              | External-layout docs describe measure functions for content whose size depends on another layout system, including text.                            | High.               |
| Yoga can avoid full-tree recomputation.                             | Incremental-layout docs describe dirty nodes, dirty ancestors, and skipping clean nodes when parent constraints have not changed.                   | High.               |
| Yoga has compatibility knobs.                                       | Config docs describe errata presets and point-scale rounding; styling docs describe default differences and `UseWebDefaults`.                       | High.               |
| Rust integration through `yoga` is a native binding, not pure Rust. | docs.rs describes `yoga` as Rust bindings for Facebook Yoga and lists build dependencies such as `bindgen` and `cc`.                                | Medium-high.        |
| Taffy is the common Rust-native alternative to evaluate.            | Taffy docs describe a Rust UI layout library implementing Flexbox, Grid, and Block layout. The `yoga` crate page also points to Taffy as pure Rust. | High.               |

## Important Terms

| Term               | Meaning                                                                                              |
| ------------------ | ---------------------------------------------------------------------------------------------------- |
| Yoga node          | One layout box in a Yoga tree.                                                                       |
| Style              | The per-node layout inputs: flex settings, dimensions, spacing, positioning, display, overflow, etc. |
| Layout result      | The computed offset and size for a node after layout calculation.                                    |
| Main axis          | The axis children are laid out along, determined by flex direction.                                  |
| Cross axis         | The axis perpendicular to the main axis.                                                             |
| Measure function   | A callback used to size a leaf node through an external system such as text layout.                  |
| Dirty node         | A node marked for recalculation because style, children, or measured content changed.                |
| Errata             | Compatibility flags that trade standards conformance against older Yoga behavior.                    |
| Point scale factor | A config value that maps Yoga's point grid to a physical pixel grid for rounding.                    |

## Core Layout Flow

1. Create a Yoga config, usually one shared config for an application or layout
   subsystem.
2. Create a root node.
3. Create child nodes and insert them into the root or other parent nodes.
4. Apply style properties to each node.
5. Optionally attach measure functions to leaf nodes whose size depends on
   external content.
6. Call layout calculation on the root with available width/height constraints.
7. Read each node's computed layout result.
8. Render or update application objects using those computed rectangles.

Yoga does not prescribe what happens after step 7. A UI framework might draw to
CoreGraphics, Android views, an HTML canvas, a terminal cell grid, or a custom
GPU renderer.

## Styling Model

Yoga's style surface resembles a typed subset of CSS:

- **Display:** flex, none, contents.
- **Direction:** layout direction for left-to-right or right-to-left behavior.
- **Flex direction:** column, row, and reverse variants.
- **Flex sizing:** flex basis, grow, shrink.
- **Wrapping:** no-wrap, wrap, wrap-reverse.
- **Alignment:** align items, align self, align content.
- **Justification:** flex-start, center, flex-end, space-between, space-around,
  space-evenly.
- **Spacing:** margin, padding, border, gap.
- **Dimensions:** width, height, min/max width, min/max height.
- **Positioning:** relative, absolute, static-like behavior depending on API and
  configuration.
- **Overflow:** visible, hidden, scroll in bindings that expose it.

Yoga acts like border-box sizing: specified dimensions include padding and
border.

## Defaults And Compatibility

Yoga's defaults intentionally differ from browser CSS in a few important ways:

- `flex-direction` defaults to column.
- `align-content` defaults to flex-start.
- `flex-shrink` defaults to 0.
- `position` defaults to relative.

`UseWebDefaults` can align some defaults with browser behavior, but not all
historical differences. Yoga also supports errata flags so applications can opt
between newer standards-conforming behavior and older Yoga-compatible behavior.

## Units And Rounding

Yoga works in arbitrary points and percentages. The host application decides
what a point means:

- mobile frameworks often map points to density-independent pixels;
- canvas renderers may map points to pixels;
- terminal renderers may map points to character cells.

Yoga can round layout to a physical pixel grid with a point scale factor. A
scale factor of `0` disables pixel-grid rounding.

## Measure Functions

Measure functions are Yoga's bridge to content it cannot size alone. Typical
uses:

- text layout;
- images with intrinsic dimensions;
- widgets owned by another layout system;
- terminal content measured in cells.

The measure function receives:

- available width and height, if constrained;
- a measure mode per axis: exactly, at-most, or undefined.

It returns the measured width and height. If the content changes, the host must
mark the node dirty so Yoga knows to measure again.

## Incremental Layout

Yoga tracks dirty nodes to avoid recomputing unaffected parts of a tree:

- The first layout visits all nodes.
- Later style or child changes dirty the changed node and its ancestors.
- During a later layout, clean subtrees can be skipped when parent constraints
  are unchanged.
- Nodes expose a new-layout flag so host frameworks can skip applying unchanged
  results.

This matters most for large UI trees. For tiny trees, the simpler approach may
still be to recompute the whole tree.

## Integration Costs

Using Yoga usually requires host code for:

- storing or mirroring an application component tree as Yoga nodes;
- translating app-level props into Yoga styles;
- mapping app units into Yoga points and back;
- attaching measure functions for text and intrinsic content;
- dirtying nodes when external content changes;
- applying computed layout results to renderable objects;
- freeing native nodes/configs where the binding requires manual cleanup;
- testing rounding and edge cases at small sizes.

Yoga removes the need to hand-write Flexbox geometry, but it does not remove the
need to design a layout/rendering architecture.

## Rust Notes

The `yoga` crate on docs.rs is a binding to Facebook's Yoga. It exposes Yoga
nodes and configs but has sparse generated documentation and native build
dependencies.

Taffy is the closest pure-Rust comparison point. It implements Flexbox, Grid,
and Block layout, has a high-level tree API and low-level embedding traits, and
is more idiomatic if the host project wants to stay fully Rust-native.

## Questions for Review

- What does Yoga compute, and what does it deliberately not compute?
- Why do text nodes usually need measure functions?
- Which Yoga defaults differ from browser CSS defaults?
- What does the host application need to do after Yoga computes layout?
- When is dirty incremental layout worth the extra architecture?

## Connections

- Related ideas: Flexbox, React Native layout, terminal cell layout, text
  measurement, Taffy, UI component trees, renderer pipelines.
- Related sources: Gridland's Yoga integration, OpenTUI, React reconciler host
  configs, terminal row models.
- Contradictions or tensions: Yoga gives mature Flexbox behavior, but it also
  introduces a second layout tree that must be kept in sync with application
  state.
- Useful applications: nested panes, dashboards, full-screen TUI component
  systems, canvas UI frameworks, native mobile UI frameworks.

## Open Questions

- How much of Yoga's behavior should a host framework expose directly versus
  hiding behind simpler component props?
- For Rust projects, when is compatibility with Yoga/React Native worth native
  bindings instead of using Taffy?
- How should terminal renderers handle wide Unicode cells if Yoga's units are
  abstract numeric points?
- Is incremental layout worth implementing for small terminal apps, or only for
  large component trees?

## Notable Quotes

> Yoga itself is not a UI framework, and does not do any drawing itself.

> Yoga instead works on "points".

## Takeaways

- Yoga is a portable Flexbox-style box layout engine, not a renderer.
- Hosts still own text measurement, drawing, event handling, and unit mapping.
- Yoga is most valuable when an application has a real component tree with
  nested boxes whose geometry would be tedious or error-prone to compute by
  hand.
