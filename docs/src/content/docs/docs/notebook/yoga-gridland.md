---
title: "Gridland's Yoga Integration"
Sources:
  - https://github.com/thoughtfulllc/gridland
Author: thoughtfulllc / Gridland maintainers
Date: 2026-07-01
Tags: [gridland, yoga, opentui, react, terminal-ui, renderer, layout]
---

## Summary

Gridland uses Yoga as the central geometry engine in a React-style terminal UI
runtime: JSX elements become renderables, renderables own Yoga nodes, React
tree mutations update the Yoga tree, Yoga computes cell-based Flexbox layout,
and the renderer paints the resulting rectangles into browser, terminal, or
headless cell buffers.

## Key Ideas

- **Yoga is in the runtime path:** `@gridland/core` depends on `yoga-layout`.
- **Renderables are layout nodes:** The base `Renderable` class creates and owns
  a Yoga node.
- **React reconciliation mirrors into Yoga:** Appending, inserting, and removing
  renderables updates both Gridland's renderable arrays and Yoga's child tree.
- **Layout runs before paint:** The root render pass calculates Yoga layout,
  copies computed rectangles into renderables, then emits render commands.
- **Gridland maps authoring props to Yoga:** Props such as `width`, `height`,
  `flexDirection`, `flexGrow`, `padding`, `margin`, `gap`, `position`, and
  `overflow` are translated into Yoga setters.
- **Yoga's units are terminal cells in Gridland:** Numeric values represent
  character columns or rows, not pixels.
- **Text measurement is external:** Text renderables install Yoga measure
  functions backed by `TextBufferView`.
- **Visual boxes feed layout:** Borders and gaps are not just drawing details;
  they are written into Yoga as border and gutter values.
- **Painting is a separate phase:** Yoga returns geometry. Browser, terminal,
  and headless renderers decide how that geometry becomes output.

## Claims & Evidence

| Claim                                                    | Support                                                                                                                                                                                          | Caveat / Confidence |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------- |
| Gridland directly uses Yoga.                             | `packages/core/package.json` lists `yoga-layout` as a dependency.                                                                                                                                | High.               |
| Yoga is attached to the base renderable abstraction.     | `Renderable.ts` imports Yoga, creates a shared config, stores `protected yogaNode`, creates a node in the constructor, and exposes `getLayoutNode`.                                              | High.               |
| Gridland intentionally uses non-web Yoga defaults.       | `Renderable.ts` sets `UseWebDefaults(false)` and `PointScaleFactor(1)`; internal layout notes state that `box` defaults to column direction.                                                     | High.               |
| JSX tree operations update the Yoga tree.                | `host-config.ts` creates renderables for intrinsic elements. `Renderable.add` and `insertBefore` call `yogaNode.insertChild`; removal calls `yogaNode.removeChild`.                              | High.               |
| Yoga computes layout before painting.                    | `RootRenderable.render` calculates layout, updates renderables from computed layout, then executes render commands. `web/src/render-pipeline.ts` follows the same sequence for browser/headless. | High.               |
| Computed Yoga layout becomes renderable geometry.        | `updateFromLayout` calls `yogaNode.getComputedLayout`, copies `left`, `top`, `width`, and `height` into renderable fields, and triggers resize behavior.                                         | High.               |
| Text sizing is delegated through Yoga measure functions. | `TextBufferRenderable.setupMeasureFunc` installs `yogaNode.setMeasureFunc`; the callback calls `textBufferView.measureForDimensions`.                                                            | High.               |
| Box borders and gaps participate in layout.              | `BoxRenderable.applyYogaBorders` calls `setBorder`; gap props call `setGap` on Yoga gutters.                                                                                                     | High.               |
| Gridland treats Yoga units as terminal cells.            | Docs say width, height, padding, margin, gap, and borders are integer character-cell units; the Flexbox section says measurements are cells.                                                     | High.               |
| Overflow clipping is layered after layout.               | Internal render-pipeline notes describe scissor stacks for `overflow="hidden"` containers; source emits push/pop scissor commands.                                                               | Medium-high.        |

## Important Terms

| Term             | Meaning                                                                                                |
| ---------------- | ------------------------------------------------------------------------------------------------------ |
| Renderable       | Gridland/OpenTUI runtime object corresponding to a JSX intrinsic element such as `box` or `text`.      |
| Yoga node        | The layout node attached to a renderable. It receives style inputs and returns computed geometry.      |
| RootRenderable   | The root layout/render object. It owns the root Yoga node and kicks off layout before painting.        |
| Cell             | A terminal character position. Gridland maps numeric layout values to columns/rows.                    |
| Measure function | A Yoga callback used by text renderables to ask `TextBufferView` how large the content should be.      |
| Render command   | A collected command such as render, push scissor rect, pop scissor rect, push opacity, or pop opacity. |
| Scissor rect     | A clipping rectangle used by the buffer/paint path to enforce overflow boundaries.                     |

## Pipeline

1. User code writes React JSX with Gridland intrinsic elements such as `box`,
   `text`, `input`, `scrollbox`, `code`, or `markdown`.
2. Gridland's React host config creates renderable instances from those element
   types.
3. Each renderable constructs a Yoga node and applies layout props to it.
4. Parent/child reconciliation mirrors insertions and removals into Yoga.
5. The root render pass calls Yoga layout for the root column/row dimensions.
6. The render pass walks the renderable tree and copies Yoga's computed
   rectangle into each renderable.
7. Renderables produce draw commands or write to an optimized cell buffer using
   their computed `x`, `y`, `width`, and `height`.
8. The target renderer turns the cell buffer into canvas pixels, terminal
   stdout, or plain text.

Gridland's docs describe the browser path as:

`React JSX -> reconciler -> Renderable tree -> Yoga layout engine -> BrowserBuffer -> CanvasPainter -> HTML5 Canvas`.

## Yoga Configuration

Gridland creates a shared Yoga config in `Renderable.ts`:

- `UseWebDefaults(false)`
- `PointScaleFactor(1)`

Practical effects:

- The default layout direction is column, matching Yoga/OpenTUI rather than the
  web developer intuition that horizontal rows are common.
- Numeric values map cleanly to terminal cells.
- Explicit numeric width or height makes the base renderable default
  `flexShrink` to `0`; otherwise shrink defaults to `1`.

## Prop Mapping

`Renderable.setupYogaProperties` maps Gridland props into Yoga:

- size: `width`, `height`, `minWidth`, `minHeight`, `maxWidth`, `maxHeight`;
- flex: `flexGrow`, `flexShrink`, `flexBasis`, `flexDirection`, `flexWrap`;
- alignment: `alignItems`, `justifyContent`, `alignSelf`;
- positioning: `position`, `top`, `right`, `bottom`, `left`;
- clipping signal: `overflow`;
- spacing: `margin`, `marginX`, `marginY`, side-specific margins, `padding`,
  `paddingX`, `paddingY`, side-specific padding.

`lib/yoga.options.ts` is the string-to-Yoga-enum adapter. It parses authoring
strings like `"row"`, `"column"`, `"center"`, `"space-between"`,
`"hidden"`, `"absolute"`, and `"wrap"` into Yoga constants.

## Cell Units

Gridland uses character cells as the layout unit:

- `width={40}` means 40 terminal columns.
- `height={3}` means 3 terminal rows.
- `padding={1}`, `margin={1}`, `gap={1}`, and border sizes all consume whole
  cells.
- Percentages are relative to the parent box.

This lets the same component tree run in a browser canvas, a real terminal, or
headless text output without converting between pixels and terminal columns.

## Text Measurement

Text renderables do not rely on Yoga to understand text. They own a
`TextBuffer` and `TextBufferView`, then install a Yoga measure function:

- Undefined width means max-content measurement with no wrapping.
- Constrained width is floored to an integer cell width before measuring.
- The measured result is at least `1x1`.
- `AtMost` width mode clamps non-absolute nodes to Yoga's available size.
- Text content, wrap mode, and size changes mark the Yoga node dirty.

This is a standard Yoga integration pattern: the layout engine asks for leaf
size, but the text system remains responsible for text-specific wrapping and
measurement.

## Box Layout And Paint

`BoxRenderable` splits geometry from drawing:

- `renderSelf` draws the visible box through the buffer.
- `applyYogaBorders` writes border thickness into Yoga edges.
- `gap`, `rowGap`, and `columnGap` write Yoga gutter values.
- `getScissorRect` adjusts clipping for border insets.

The same visual property can therefore affect both layout and painting.

## Render Targets

Gridland supports multiple render targets from the same component tree:

- Browser: an HTML canvas via `BrowserBuffer` and `CanvasPainter`.
- Terminal: stdout through the native OpenTUI/Bun path.
- Headless: plain text for agents, crawlers, screen readers, and snapshots.

The headless renderer still runs the same layout pipeline. It creates a cell
buffer, creates a render context with fixed columns/rows, calculates layout,
renders once, and converts the buffer to text.

## Architectural Lessons

- Yoga is most natural when the application already has a component tree.
- The component tree and Yoga tree must stay synchronized.
- Text requires its own measurement subsystem even when Yoga owns box layout.
- Rendering remains target-specific after layout.
- Clipping, hit testing, opacity, focus, and z-order live around Yoga rather
  than inside Yoga.
- A cell-based UI can use Yoga by defining Yoga points as cells.

## Questions for Review

- What makes Yoga central to Gridland's architecture rather than an optional
  helper?
  - Treat Yoga as central only when the UI is represented as a reusable component tree needing Flexbox layout.
- How does Gridland synchronize the React renderable tree with the Yoga tree?
  - Keep renderable-tree mutations and Yoga-node mutations in the same host operations so layout cannot drift from component state.
- Why does text need a measure function instead of being handled directly by
  Yoga?
  - Provide explicit text measurement because Yoga computes boxes but does not know terminal cell width or text wrapping.
- How do borders and gaps affect both layout and paint?
  - Account for borders and gaps in layout before painting so visual cells match computed geometry.
- Why can the same Yoga-computed component tree target browser, terminal, and
  headless output?
  - Share layout geometry but keep painting, clipping, and text measurement target-specific.

## Connections

- Related ideas: Flexbox layout in cell units, React reconciler host config,
  OpenTUI renderables, headless rendering, native terminal rendering, scissor
  clipping, text measurement callbacks.
- Related sources: `docs/internal/notes/yoga.md`, OpenTUI, React Native layout,
  browser canvas renderers.
- Contradictions or tensions: Yoga simplifies nested box geometry but requires a
  parallel layout tree, explicit text measurement, and target-specific painting.
- Conceptual uses: full-screen TUI frameworks, browser-rendered terminal
  UIs, reusable component libraries, headless rendering of component trees.

## Open Questions

- How much of this design comes from upstream OpenTUI versus Gridland-local
  additions?
  - **Recommendation**: Treat Yoga/Gridland as a reference for full component trees and multi-target layout, not as a dependency for simple row-first terminal rendering.
- Does Gridland ever bypass Yoga for fixed row output, or is all visible output
  rooted in the renderable tree?
  - **Recommendation**: Keep fixed row output outside Yoga unless the UI is already committed to a component tree.
- How does Gridland handle wide Unicode cells and ambiguous-width characters
  across browser and terminal targets?
  - **Recommendation**: Treat browser and terminal width behavior as separate measurement policies even when they share a layout tree.
- Could the browser/headless path share more incremental layout state instead of
  recalculating layout each render?
  - **Recommendation**: Add incremental layout sharing only after profiling shows full recalculation is a real bottleneck.

## Notable Quotes

> Layout in Gridland uses Yoga, Facebook's C++ Flexbox engine.

> Everything is measured in character cells, not pixels.

## Takeaways

- Gridland uses Yoga deeply: renderables own Yoga nodes and root rendering starts
  by calculating the Yoga tree.
- The integration pattern is Yoga plus a component tree, cell units, text measure
  functions, and a separate buffer/paint pipeline.
- Yoga does not replace Gridland's renderer; it supplies geometry that the
  renderer uses for canvas, terminal, and headless output.
