---
title: "Terminal UI"
---

This page explains how thndrs separates finalized terminal history from the
mutable live surface.

## Mental Model

Finalized transcript blocks belong to native terminal scrollback. thndrs owns
only the mutable live surface, including streaming output, the composer,
status, pickers, and detail panes.

## Responsibilities

## Semantic Views

## Transcript Rendering

The inline coordinator commits each finalized transcript block to terminal
history once. The application does not reconstruct committed history on every
frame.

## Live Surface Rendering

Ratatui draws the bounded live surface in one terminal transaction. The
renderer-owned row model makes wrapping, padding, styling, and cursor placement
testable without terminal I/O.

## Terminal Lifecycle

## Boundaries

## Key Types

## Invariants

- Finalized history is committed once to the terminal.
- Mutable state remains inside the live surface.
- Terminal I/O does not belong in the row model.

## Source Map

## Related
