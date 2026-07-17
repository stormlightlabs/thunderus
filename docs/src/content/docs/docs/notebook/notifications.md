---
title: "Desktop Notifications From Terminal Apps"
Author: Freedesktop.org, Canonical, Apple, Microsoft, iTerm2, kitty, notify-rust, terminal-notifier, BurntToast
Date: 2026-07-03
Captured: 2026-07-03
Tags: [terminal, cli, notifications, desktop, xdg, macos, windows, tui]
Sources:
  - https://specifications.freedesktop.org/notification/latest/
  - https://manpages.ubuntu.com/manpages/noble/man1/notify-send.1.html
  - https://developer.apple.com/library/archive/documentation/LanguagesUtilities/Conceptual/MacAutomationScriptingGuide/DisplayNotifications.html
  - https://www.manpagez.com/man/1/osascript/
  - https://github.com/julienXX/terminal-notifier
  - https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content
  - https://www.powershellgallery.com/packages/BurntToast/1.1.0
  - https://github.com/Windos/BurntToast
  - https://docs.rs/notify-rust/latest/notify_rust/
  - https://github.com/hoodie/notify-rust
  - https://iterm2.com/documentation-escape-codes.html
  - https://sw.kovidgoyal.net/kitty/kittens/notify/
  - https://sw.kovidgoyal.net/kitty/desktop-notifications/
---

## Summary

Terminal apps trigger desktop notifications through one of three paths:
directly calling the platform notification API, shelling out to a platform
helper, or emitting a terminal escape sequence that asks the terminal emulator
to post the notification on the app's behalf.

## Key Ideas

- **Desktop notification systems are session services:** On Linux/BSD desktops,
  the Freedesktop spec defines a single session-scoped D-Bus service named
  `org.freedesktop.Notifications`. A client sends `Notify` requests to that
  service and receives IDs, close signals, and sometimes action signals.
- **Command-line helpers are thin platform adapters:** `notify-send` wraps the
  Freedesktop notification service for shell use. `osascript` can run Apple's
  Standard Additions `display notification` command. `terminal-notifier` wraps
  macOS User Notifications as a command-line app bundle. BurntToast wraps
  Windows toast notifications for PowerShell.
- **Terminal escape protocols proxy through the emulator:** iTerm2 supports a
  legacy `OSC 9` notification escape code. kitty defines a richer notification
  protocol and exposes it through `kitten notify`, which can work over SSH
  because the remote process only writes bytes to the local terminal.
- **Capabilities are uneven:** Expiration, icons, sounds, urgency, actions,
  persistence, notification replacement, and click callbacks are all
  platform- or server-dependent. Robust CLIs treat them as best-effort features.
- **Identity matters:** Desktop systems often attribute the notification to a
  sender application. macOS adds the script or app bundle to Notification
  settings after it posts. Windows app notifications are tied to app identity,
  and Microsoft's quickstart notes that elevated admin apps are not supported.
- **Rust abstractions exist but do not erase backend differences:** `notify-rust`
  exposes one Rust API across XDG, macOS, and Windows, but documents several
  no-op, ignored, or backend-specific methods.

## Claims & Evidence

| Claim                                                                                           | Support                                                                                                                                                                                                            | Caveat / Confidence                                                                                                    |
| ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| Linux/BSD desktop notifications are best modeled as D-Bus calls, not terminal rendering.        | The Freedesktop spec says conforming implementations expose `org.freedesktop.Notifications` on the session bus and define `Notify`, `CloseNotification`, capabilities, close signals, and action signals.          | High. Availability is not guaranteed; clients must handle no server or missing capabilities.                           |
| `notify-send` is the common shell-level wrapper for Freedesktop notifications.                  | Its man page describes sending desktop notifications through a notification daemon and exposes summary/body, app name, actions, urgency, timeout, icon, category, hints, replace ID, wait, and transient mode.     | High. The man page also states that some desktops ignore the requested expiration time.                                |
| Actions and callbacks are optional on Freedesktop servers.                                      | The spec says clients should check the `actions` capability and must not assume `ActionInvoked` will be emitted.                                                                                                   | High. Use actions only when losing the callback is acceptable.                                                         |
| Urgency is only a hint until the server chooses presentation behavior.                          | The Freedesktop spec defines low, normal, and critical urgency; it says low/normal display is server-defined and critical should not auto-expire.                                                                  | High. Real servers may still apply user policy.                                                                        |
| macOS CLIs can use AppleScript through `osascript` for simple notifications.                    | Apple's guide documents `display notification` with body text, title, subtitle, and sound; `osascript` runs AppleScript or other OSA scripts from command-line arguments, files, or stdin.                         | High. It is simple, but attribution and click behavior are tied to the script or host app.                             |
| `terminal-notifier` exists because macOS notification behavior is app-bundle shaped.            | Its README says it is packaged as an application bundle and documents title, subtitle, sound, group replacement/removal, URL opening, app activation, sender, icons, and click execution.                          | Medium-high. The project is still widely referenced, but its latest listed release is from 2017.                       |
| Windows toast notifications are richer but more app-identity oriented than simple Unix helpers. | Microsoft documents XML/AppNotificationBuilder content with text, images, actions, inputs, sounds, timestamps, and progress; the quickstart says Windows App SDK apps send and respond to local app notifications. | High. A plain portable CLI may need a helper, shortcut, AppUserModelID, or library wrapper rather than raw shell text. |
| BurntToast is the practical PowerShell route on Windows.                                        | PowerShell Gallery lists BurntToast as a module for creating and displaying Windows 10 toast notifications, with functions for text, images, buttons, progress bars, history, submit, update, and removal.         | High for PowerShell users. It is an external module dependency, not built into Windows.                                |
| Terminal-emulator notification protocols are useful for remote sessions.                        | kitty's `kitten notify` docs explicitly say it works over SSH, and its protocol defines escape-code payloads for title, body, urgency, identifiers, actions, icons, sounds, and close/activation responses.        | High for terminals that implement the protocol. Other terminals will ignore or mishandle unknown private escapes.      |
| iTerm2's notification escape is simple but proprietary.                                         | iTerm2 documents `OSC 9 ; [message] ST` for posting a notification and warns that its non-standard escape codes may not work in tmux, screen, or other emulators.                                                  | High. Treat it as terminal-specific, not a cross-platform notification API.                                            |
| Cross-platform libraries still require feature detection and fallback behavior.                 | `notify-rust` documents platform differences: some methods are XDG-only, macOS-only, ignored, or no-ops; it recommends `target_os` toggles for unavailable methods.                                                | High. A clean local API should expose fewer guarantees than the richest backend.                                       |

## Important Terms

| Term                 | Meaning                                                                                                                                         |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Desktop notification | A passive system popup used to report an event without taking over the user's current task.                                                     |
| Notification server  | The session service that receives notification requests and decides how to display them.                                                        |
| D-Bus                | The message bus used by the Freedesktop notification protocol on Linux/BSD desktops.                                                            |
| Summary              | The short notification title or headline.                                                                                                       |
| Body                 | Optional longer text shown below the summary.                                                                                                   |
| Replaces ID          | A notification ID used to update an existing notification instead of creating another popup.                                                    |
| Hint                 | Optional metadata for a notification server, such as urgency, category, image path, sound, or transient behavior.                               |
| Urgency              | Low, normal, or critical importance. It influences display policy but is not full control over presentation.                                    |
| Action               | A user-invokable response, usually a click or button, that may produce a callback to the sender.                                                |
| App identity         | The OS-visible application name, bundle identifier, desktop entry, shortcut identity, or sender used for attribution and notification settings. |
| OSC                  | Operating System Command, a class of terminal escape sequence used by some emulators for non-text features.                                     |

## Mechanisms

### Linux/BSD XDG

The native mechanism is the Freedesktop Desktop Notifications protocol over the
session bus. A program can call `org.freedesktop.Notifications.Notify` directly
through a D-Bus library, use a wrapper such as `notify-send`, or use a higher
level library such as `notify-rust`.

Useful guarantees:

- `Notify` returns an ID that can be used for later close or replacement.
- Servers publish capabilities, so clients can check for actions, body markup,
  persistence, sound, static icons, hyperlinks, and related features.
- `CloseNotification` gives clients a way to remove stale notifications.

Limits:

- Notification servers are not required to be present.
- Hints are optional and unknown hints must be ignored.
- Actions and activation tokens are optional.
- Expiration behavior is partly server policy and partly user policy.

### macOS

The lowest-friction CLI path is:

```sh
osascript -e 'display notification "Build finished" with title "thndrs"'
```

That uses AppleScript's Standard Additions through the `osascript` command.
For richer shell use, `terminal-notifier` provides a dedicated command-line
wrapper around macOS User Notifications with grouping, icons, sounds, URLs, app
activation, and notification removal.

Limits:

- The notification is attributed to the script host or app bundle, not
  necessarily the terminal program's logical product name.
- User Notification settings decide banner vs alert behavior.
- Click behavior can relaunch the displaying app, which matters for script apps.
- Some richer `terminal-notifier` options rely on private or sender-specific
  behavior.

### Windows

Modern Windows notifications are app notifications/toasts. The platform content
model is XML-shaped and can include text, images, action buttons, inputs, audio,
timestamps, progress, headers, and activation arguments.

For CLI use, the practical routes are:

- call a Windows notification library from the app;
- use PowerShell with BurntToast;
- use a helper executable or shortcut with the correct AppUserModelID/App
  identity;
- use a cross-platform library that wraps Windows notification APIs.

Limits:

- App identity and shortcut registration affect attribution and whether toasts
  work correctly.
- Microsoft's Windows App SDK quickstart says app notifications are not
  supported for elevated admin apps.
- Rich click handling assumes the app can receive activation and parse arguments.

### Terminal Emulator Protocols

Terminal protocols are different from OS APIs: the CLI writes escape sequences
to stdout/stderr, and the terminal emulator turns them into a local notification.
This is attractive when the app runs over SSH because the remote process does not
need access to the local desktop notification bus.

iTerm2:

```sh
printf '\033]9;Build finished\a'
```

kitty:

```sh
kitten notify --urgency normal "Build finished" "Tests passed"
```

kitty also exposes `--only-print-escape-code`, which lets another program embed
the protocol without shelling out for display behavior.

Limits:

- Escape support is terminal-specific.
- Multiplexers such as tmux or screen may block, rewrite, or fail to pass through
  private escape codes.
- Unknown escape sequences can be ignored, but terminal-specific behavior should
  never be required for correctness.

## Design Lessons

- Prefer a small internal notification API with fields most systems can support:
  title, body, urgency, timeout policy, replacement key, and optional open/action
  intent.
- Treat notification delivery as best effort. Failure to notify should not fail
  the underlying CLI operation.
- Feature-detect when possible: Freedesktop capabilities, `notify-rust` platform
  support, helper presence on `PATH`, terminal identity, or explicit user config.
- Keep click actions optional. A command that needs user input should still
  present the next step in the terminal.
- Support replacement/update to avoid notification spam for long-running jobs.
- Respect user attention: notify on completion, failure, blocked user input, or
  long-running background state changes; do not notify for every stream event.
- Expose a "never", "auto", or "terminal-only" setting. Some users want no
  desktop notifications from terminal tools.
- Escape-based notification should be an extra backend, not the only backend.
  It is strongest for SSH and local terminal integration, weakest for broad
  desktop support.

## Questions For Review

- What are the three main ways a terminal app can trigger a desktop notification?
  - Direct OS API calls, command/helper wrappers, and terminal escape sequences.
- Why should a CLI treat notification actions as optional?
  - Several notification servers and terminal emulators may ignore actions or not
    send callback signals.
- Why are terminal escape notifications useful over SSH?
  - The remote CLI only emits bytes; the local terminal emulator posts the local
    desktop notification.
- What local fields are worth standardizing before choosing a backend?
  - Title, body, urgency, timeout policy, replacement key, and optional action or
    open intent.
- When should a CLI avoid sending a notification?
  - When the event is already visible and immediate, when the user disabled
    notifications, or when repeated notifications would create noise.

## Connections

- Related ideas: long-running command completion, background agent tasks,
  streaming model runs, terminal focus state, shell integration, and remote SSH
  workflows.
- Related sources: Freedesktop notification spec, `notify-send`, Apple's
  Notification Center scripting docs, Windows App SDK notification docs, kitty
  notification protocol, iTerm2 proprietary escape codes, `notify-rust`.
- Contradictions or tensions: Native OS APIs have better desktop integration but
  are platform-specific; terminal escape protocols work well over SSH but are
  emulator-specific.
- Useful applications: alert when an agent run finishes, when approval/input is
  needed, when a long-running test/build completes, or when a background job
  fails while the terminal is unfocused.

## Open Questions

- Which terminal emulators besides kitty and iTerm2 should be treated as explicit
  notification backends?
- Should notification behavior depend on focus/visibility when the terminal
  emulator can report or enforce it?
- How should a portable CLI represent click actions without promising they work
  everywhere?
- Should a Rust CLI use `notify-rust`, platform-specific backends, or shell
  helpers first?
- What is the right fallback order: terminal escape, platform API, helper
  command, or no-op?

## Takeaways

- Desktop notifications are external integration, not terminal UI; design them
  as best-effort side effects with clear fallback.
- The most portable abstraction is intentionally small because backend feature
  parity is poor.
- Terminal-emulator notification protocols are especially valuable for SSH and
  focused terminal workflows, but they should complement native OS backends
  rather than replace them.
