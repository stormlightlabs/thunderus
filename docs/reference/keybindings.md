---
outline: deep
---

# Keybindings

Complete reference for Thunderus TUI keyboard shortcuts.

## Composer (Input)

| Key          | Action                         |
| ------------ | ------------------------------ |
| `Enter`      | Send message (if non-empty)    |
| `Esc`        | Clear input buffer             |
| `Backspace`  | Delete character before cursor |
| `Delete`     | Delete character at cursor     |
| `Left/Right` | Move cursor                    |
| `Home/End`   | Jump to start/end of input     |
| `Up/Down`    | Browse message history         |
| `Tab`        | Autocomplete file paths        |
| `!cmd`       | Execute shell command          |
| `@`          | Open fuzzy file finder         |
| `/`          | Start slash command            |

## Transcript Navigation

| Key           | Action                          |
| ------------- | ------------------------------- |
| `j/k`         | Navigate between cards / scroll |
| `g`           | Jump to top                     |
| `G`           | Jump to bottom                  |
| `Ctrl+U`      | Page up                         |
| `Space/Enter` | Expand/collapse focused card    |
| `v`           | Toggle verbose mode for card    |

## Layout Controls

| Key       | Action                           |
| --------- | -------------------------------- |
| `Ctrl+S`  | Toggle sidebar                   |
| `[` / `]` | Collapse/expand sidebar sections |
| `Ctrl+L`  | Clear transcript view            |
| `Ctrl+T`  | Toggle color theme               |

## Agent Control

| Key            | Action                   |
| -------------- | ------------------------ |
| `Ctrl+C`       | Cancel generation / quit |
| `Ctrl+D`       | Exit TUI                 |
| `Ctrl+R`       | Retry last failed action |
| `Ctrl+Shift+G` | Open external editor     |

## Approval Prompts

| Key | Action         |
| --- | -------------- |
| `y` | Approve action |
| `n` | Reject action  |
| `c` | Cancel task    |

## Detail Levels

Cards support three expansion levels:

1. **Collapsed (default)**: Intent + outcome summary
2. **Expanded**: Detailed context and metadata
3. **Verbose**: Full logs, reasoning chain, trace
