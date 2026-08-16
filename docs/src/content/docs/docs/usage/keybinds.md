---
title: "Keybindings"
---

This page lists the keyboard shortcuts available in the thndrs TUI.

Shortcuts are grouped by the context in which they apply.

Many editing shortcuts follow common [readline](https://man7.org/linux/man-pages/man3/readline.3.html)-style conventions.

## Global

These shortcuts work from any mode unless otherwise noted.

| Key                | Description                                                                      |
| ------------------ | -------------------------------------------------------------------------------- |
| `Ctrl+C`           | Cancel a running stream; quit when idle                                          |
| `Ctrl+D`, `Ctrl+D` | Show quit confirmation; press again to quit                                      |
| `Ctrl+G`           | Send the running draft as steering guidance                                      |
| `Enter`             | Queue the running draft as a follow-up                                           |
| `Ctrl+O`           | Open focused detail for failed/truncated tool output, diffs, warnings, or errors |
| `?`                | Open help overlay (only when the prompt is empty)                                |
| `:`                | Enter command mode (only when idle or after an error)                            |

## Prompt Input

Cursor movement and text editing while in the prompt.

| Key                                   | Description                                            |
| ------------------------------------- | ------------------------------------------------------ |
| `Left` or `Ctrl+B`                    | Move cursor left                                       |
| `Right` or `Ctrl+F`                   | Move cursor right                                      |
| `Alt+Left`, `Ctrl+Left`, or `Alt+B`   | Move cursor to the start of the previous word          |
| `Alt+Right`, `Ctrl+Right`, or `Alt+F` | Move cursor to the end of the next word                |
| `Home` or `Ctrl+A`                    | Move cursor to the start of the line                   |
| `End` or `Ctrl+E`                     | Move cursor to the end of the line                     |
| `Tab`                                 | Accept the active suggestion (`:` command or `@` path) |
| `Shift+Enter` or `Ctrl+J`             | Insert a newline in a multi-line prompt                |
| `Ctrl+T`                              | Transpose the adjacent characters                      |
| `Enter`                               | Submit the current prompt                              |
| `Backspace`                           | Delete the character before the cursor                 |
| `Delete`                              | Delete the character after the cursor (forward delete) |

## History

Move through previously submitted prompts.

| Key    | Description                     |
| ------ | ------------------------------- |
| `Up`   | Recall older input from history |
| `Down` | Recall newer input from history |

## Transcript Scrollback

Completed transcript entries use your terminal emulator's normal scroll,
selection, and copy controls.

## Help Overlay

Available while the help overlay is open.

| Key                    | Description                             |
| ---------------------- | --------------------------------------- |
| `Esc`                | Close help overlay and return to prompt |
| `Up` / `Down`        | Scroll the help rows                         |

## Command Mode

Available while typing a `:` command.

Commands currently include `clear`, `quit`, `exit`, `help`, `bg`, `bg cancel <id>`, `model`,
`skills`, `doctor`, `auth status`, `config path`, `config show`, `setup`,
`login`, and `logout`.

| Key                     | Description                                         |
| ----------------------- | --------------------------------------------------- |
| `Esc`                   | Cancel command and return to prompt                 |
| `Backspace`             | Delete last character, or cancel command if empty   |
| `Enter`                 | Execute the typed command                           |
| `Tab`                   | Accept completion for the active command suggestion |
| Any printable character | Append to the command buffer                        |

## File Mentions

Type `@` in the prompt to open file and directory suggestions.

| Key                     | Description                                                   |
| ----------------------- | ------------------------------------------------------------- |
| `Up` / `Down`           | Move selection up / down                                      |
| `PageUp` / `PageDown`   | Move selection up / down by a page                            |
| `Left` / `Right`        | Move through the path text in the prompt and refresh results  |
| `Enter` or `Tab`        | Insert the selected path into the prompt                      |
| `Esc`                   | Close the suggestions without inserting                       |
| `Backspace` / `Delete`  | Edit the path text in the prompt and refresh results          |
| Any printable character | Insert into the prompt and filter results                     |

## Model Picker

Available after running `:model`.

| Key                     | Description                                     |
| ----------------------- | ----------------------------------------------- |
| `Up` / `Down`           | Move selection up / down                        |
| `PageUp` / `PageDown`   | Move selection up / down by a page              |
| `Enter`                 | Switch to the selected model                    |
| `Esc`                   | Close the model picker without changing models  |
| `Backspace`             | Remove the last character from the picker query |
| Any printable character | Append to the picker query and filter results   |
