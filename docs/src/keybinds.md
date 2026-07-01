# Keybindings

This page lists the keyboard (and mouse) shortcuts available in the thndrs TUI.

Shortcuts are grouped by the context in which they apply.

Many editing shortcuts follow common [readline](https://man7.org/linux/man-pages/man3/readline.3.html)-style conventions.

## Global

These shortcuts work from any mode unless otherwise noted.

| Key                | Description                                                |
| ------------------ | ---------------------------------------------------------- |
| `Ctrl+C`           | Quit immediately                                           |
| `Ctrl+D`, `Ctrl+D` | Show quit confirmation; press again to quit                |
| `Ctrl+T`           | Toggle the running input target (`steering` / `follow-up`) |
| `?`                | Open help overlay (only when the prompt is empty)          |
| `:`                | Enter command mode (only when idle or after an error)      |

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
| `Shift+Enter` or `Ctrl+J`             | Insert a newline in a multi-line prompt                |
| `Enter`                               | Submit the current prompt                              |
| `Backspace`                           | Delete the character before the cursor                 |
| `Delete`                              | Delete the character after the cursor (forward delete) |

## History

Move through previously submitted prompts.

| Key    | Description                     |
| ------ | ------------------------------- |
| `Up`   | Recall older input from history |
| `Down` | Recall newer input from history |

## Transcript Scrolling

Scroll the conversation transcript. These work even while the agent is running
so that quit/cancel shortcuts remain usable.

| Key          | Description                                            |
| ------------ | ------------------------------------------------------ |
| `PageUp`     | Scroll transcript up by 10 lines                       |
| `PageDown`   | Scroll transcript down by 10 lines (or jump to newest) |
| `Ctrl+Alt+U` | Scroll transcript up by 10 lines                       |
| `Ctrl+Alt+D` | Scroll transcript down by 10 lines                     |
| `Ctrl+Alt+Y` | Scroll transcript up by one line                       |
| `Ctrl+Alt+E` | Scroll transcript down by one line                     |

## Help Overlay

Available while the help overlay is open.

| Key                    | Description                             |
| ---------------------- | --------------------------------------- |
| `Esc`, `?`, or `Enter` | Close help overlay and return to prompt |

## Command Mode

Available while typing a `:` command.

| Key                     | Description                                       |
| ----------------------- | ------------------------------------------------- |
| `Esc`                   | Cancel command and return to prompt               |
| `Backspace`             | Delete last character, or cancel command if empty |
| `Enter`                 | Execute the typed command                         |
| Any printable character | Append to the command buffer                      |

## File Picker

Available while the file picker overlay is open.

| Key                     | Description                                     |
| ----------------------- | ----------------------------------------------- |
| `Ctrl+P`                | Open the file picker (from prompt)              |
| `Up` / `Down`           | Move selection up / down                        |
| `PageUp` / `PageDown`   | Move selection up / down by a page              |
| `Enter`                 | Insert the selected path into the prompt        |
| `Esc`                   | Close the file picker without inserting         |
| `Backspace`             | Remove the last character from the picker query |
| Any printable character | Append to the picker query and filter results   |

## Mouse

When mouse input is enabled:

| Action        | Description                          |
| ------------- | ------------------------------------ |
| `Scroll Up`   | Scroll the transcript or picker up   |
| `Scroll Down` | Scroll the transcript or picker down |
