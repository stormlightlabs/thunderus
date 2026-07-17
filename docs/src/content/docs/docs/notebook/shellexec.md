---
title: "Shell Execution Safeguards"
Source: https://github.com/agentify-sh/safeexec
Author: agentify-sh, waml, contributors
Date: 2026-04-16
Captured: 2026-07-04
Tags: [shell, execution, safeguards, coding-agent, safety, bash]
Sources: >
  - https://github.com/agentify-sh/safeexec
  - https://raw.githubusercontent.com/agentify-sh/safeexec/main/README.md
  - https://raw.githubusercontent.com/agentify-sh/safeexec/main/safeexec.sh
  - https://api.github.com/repos/agentify-sh/safeexec
  - https://api.github.com/repos/agentify-sh/safeexec/commits/main
---

## Summary

SafeExec is a Bash-based command interception layer for local human and AI-agent
shell sessions: it gates a small set of destructive command shapes with a real
TTY confirmation challenge, while explicitly remaining a guardrail rather than
a sandbox or malware-containment boundary.

## Key Ideas

- **Gate consequences, not every command:** SafeExec wraps common binaries but
  only prompts for command shapes that are likely to discard work or mutate a
  system aggressively, such as `rm -rf`, selected destructive `git` operations,
  and package-manager `audit fix --force` forms.
- **Use a real terminal for confirmation:** The confirmation path tries to read
  from `/dev/tty`, falls back only when stdin is an actual TTY, and blocks when
  no usable terminal input exists.
- **Challenge text reduces wrong-context approval:** Prompts require
  `confirm <token>`, not a fixed yes/no string. This makes stale prompts,
  background jobs, and copied confirmations less likely to run the wrong
  command.
- **Foreground checks matter for agents:** On Linux, the wrappers inspect
  `/proc/$$/stat` and refuse confirmation when the process group is not the
  controlling terminal's foreground process group.
- **PATH shims are useful but incomplete:** Soft mode installs wrappers and
  shims under `/usr/local/safeexec/bin` and `/usr/local/bin`; it can cover normal
  shell resolution but not all absolute-path or `command -p` invocations.
- **Hard mode moves closer to the binary boundary:** Ubuntu/Debian/WSL hard
  mode uses `dpkg-divert` for `/usr/bin/rm` and `/usr/bin/git`, so absolute
  paths and minimal PATH environments are harder to use as bypasses.
- **Bypass is explicit, not impossible:** SafeExec documents scoped bypasses
  (`SAFEEXEC_DISABLED=1`), per-user/global toggles, backup binaries ending in
  `.safeexec.real`, and hard-mode uninstall paths.
- **Agent policy is part of the safeguard:** The README recommends telling
  agents never to type confirmation tokens, never to use bypass mechanisms
  automatically, and to stop for human approval when `[SAFEEXEC]` appears.

## Claims & Evidence

| Claim                                                                            | Support                                                                                                                                                                                                       | Caveat / Confidence                                                                                            |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| SafeExec is a guardrail, not a sandbox.                                          | The README explicitly scopes it to accidental destructive commands in local human and AI terminal sessions, and excludes sandbox escape prevention, malware containment, and full host isolation.             | High. This is the core scope statement.                                                                        |
| TTY confirmation blocks common non-interactive bypasses.                         | The wrappers probe-open `/dev/tty`, read the response from the chosen TTY input, and block with exit code `126` if no usable TTY exists.                                                                      | High for stdin-pipe bypasses; not a hard boundary against an agent that can type into the foreground terminal. |
| A tokenized confirmation is safer than a static prompt.                          | `gen_challenge` uses `/dev/urandom` when available, with a checksum fallback, and `confirm_or_die` requires the exact `confirm <token>` string.                                                               | Medium-high. It reduces accidental confirmation reuse but does not authenticate intent.                        |
| Background or detached execution should not be allowed to consume confirmations. | `is_foreground_tty` compares process group and foreground terminal process group on Linux before prompting.                                                                                                   | Medium. The code treats unavailable `/proc` data as allow, so this is best-effort and platform-dependent.      |
| Soft shims improve normal command coverage but remain bypassable.                | Installation creates wrappers for `rm`, `git`, `npm`, `yarn`, `pnpm`, and `bun`, plus `/usr/local/bin` shims and sudo `secure_path` configuration. The README documents absolute-path bypasses for soft mode. | High. PATH-based interception is inherently weaker than binary diversion.                                      |
| Hard mode covers more agent harness cases on Debian-like systems.                | The README and script describe `dpkg-divert` for `rm` and `git`, replacing `/usr/bin/<cmd>` with dispatchers into SafeExec wrappers while storing real binaries as `.safeexec.real`.                          | High for Ubuntu/Debian/WSL; not portable to macOS or non-dpkg Linux.                                           |
| The command allow/gate rules are intentionally narrow.                           | `rm` gates only when both recursive and force flags are present; `git` gates reset/revert/checkout/restore, forced clean/switch, and stash drop/clear/pop; package managers gate audit-fix plus force.        | High. Narrow rules reduce friction but leave other risky commands outside coverage.                            |
| Operational toggles are both useful and risky.                                   | `safeexec -off`, `safeexec -on`, global state files, and `SAFEEXEC_DISABLED=1` are first-class features for automation and emergency use.                                                                     | High. These mechanisms are necessary escape valves but must be denied to autonomous agents by policy.          |

## Important Terms

| Term                     | Meaning                                                                                                                              |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| Soft mode                | PATH/shim-based interception through `/usr/local/safeexec/bin` and `/usr/local/bin`.                                                 |
| Hard mode                | Debian-family binary diversion using `dpkg-divert` so selected system paths dispatch through SafeExec.                               |
| TTY gate                 | A confirmation prompt read from a terminal device rather than normal stdin.                                                          |
| Challenge token          | A one-time token included in the required confirmation phrase.                                                                       |
| Foreground process group | The process group currently allowed to receive terminal input; SafeExec uses it to reject background confirmation attempts on Linux. |
| `.safeexec.real`         | Backup name for the original binary or symlink target that SafeExec wraps or diverts.                                                |
| Scoped bypass            | A deliberate one-command bypass such as `SAFEEXEC_DISABLED=1 <command>`.                                                             |

## Command Coverage

SafeExec gates these command families:

- `rm` when both force and recursive flags are present, including combined short
  flags like `-rf` or `-fr`.
- `git reset`, `git revert`, `git checkout`, and `git restore`.
- `git clean` with `-f` or `--force`.
- `git switch` with `-f`, `--force`, or `--discard-changes`.
- `git stash drop`, `git stash clear`, and `git stash pop`.
- `npm`, `yarn`, `pnpm`, and `bun` when the argument sequence includes
  `audit fix` or `audit --fix` plus `--force`/`-f`. Yarn also checks
  `yarn npm audit fix` forms.

Notably, this is not a general shell policy engine. It does not attempt to parse
arbitrary shell scripts, redirection, `find -exec`, `dd`, `chmod -R`,
cloud-provider CLIs, database commands, or project-specific destructive tools.

## Design Lessons

- Prefer small, legible gates over broad shell rewriting. SafeExec is easy to
  reason about because each wrapper decides on concrete command shapes.
- Put safety at the execution boundary, not just in the prompt. AGENTS.md policy
  helps, but the wrapper is the mechanism that interrupts a command.
- Make denial the default when interaction is ambiguous. If no usable TTY exists,
  blocking is safer than printing a prompt that cannot be answered correctly.
- Separate normal automation from emergency bypass. Bypasses should be explicit,
  scoped, auditable, and unavailable to autonomous agents by policy.
- Treat PATH wrapping as a convenience layer. For higher assurance, safeguards
  must account for absolute paths, privileged execution, shell builtins, command
  lookup caches, package-manager shims, and platform-specific binary locations.
- Log both blocked and confirmed events. SafeExec uses `logger` when available,
  which supports post-hoc auditing without making logging part of the decision.

## Inferences

- A robust agent harness should combine typed tool permissions with command
  interception. SafeExec catches a few dangerous shell cases, but harness-level
  allow/ask/deny rules are still needed for commands outside its wrappers.
- Confirmation tokens should be human-only. If an agent is allowed to type the
  token, SafeExec becomes friction rather than control.
- System-specific installation is part of the threat model. macOS Homebrew,
  sudo `secure_path`, WSL terminal behavior, and Debian `dpkg-divert` all change
  whether a wrapper actually sees the command.
- The narrow-gate approach favors developer ergonomics over complete coverage.
  That is a reasonable product choice for accidental foot-guns, but not enough
  for untrusted code execution.

## Questions for Review

- What is the difference between a shell guardrail and a sandbox?
- Why does SafeExec read confirmation from `/dev/tty` instead of stdin?
- Which command patterns does SafeExec gate, and which risky shell patterns does
  it intentionally leave outside scope?
- Why is soft mode easier to bypass than hard mode?
- Why should an AGENTS.md policy forbid agents from typing confirmation tokens?
- What failure mode is addressed by checking the foreground process group?
- When is an explicit scoped bypass legitimate, and what should an agent do
  before using one?

## Connections

- Related ideas: command approval gates, least-privilege tools, typed shell
  wrappers, foreground human confirmation, audit logging, runtime interruption.
- Related sources: [agents-md](/docs/notebook/agents-md/),
  [fs-traversal](/docs/notebook/fs-traversal/),
  [harness-engineering](/docs/notebook/harness-engineering/).
- Contradictions or tensions: developers want fast shell access, but agent
  harnesses need commands to be interruptible, scoped, logged, and recoverable.
- Useful applications: protect local worktrees from accidental destructive
  commands, define human-only confirmation policy, and decide where a harness
  needs typed tools instead of raw shell execution.

## Open Questions

- Should an agent harness maintain its own destructive-command classifier, or
  delegate this class of protection to tools like SafeExec?
  - **Recommendation**: Use harness permissions as the primary policy boundary
    and treat SafeExec-style interception as defense in depth for raw shell
    escape hatches.
- How should project-specific destructive commands be modeled?
  - **Recommendation**: Keep a configurable deny/ask list for project commands
    that can delete data, rewrite history, deploy, rotate secrets, or mutate
    external services.
- Should the prompt phrase include command context, not only a token?
  - **Recommendation**: Consider requiring the user to confirm both a token and
    a short command digest for especially destructive operations.
- How should safeguards work on native Windows shells?
  - **Recommendation**: Treat native PowerShell/CMD as a separate design problem;
    WSL shell wrapping does not automatically protect Windows-native execution.
- What audit data is enough without leaking sensitive command arguments?
  - **Recommendation**: Log command family, decision, cwd, timestamp, and a
    redacted command string; avoid storing secrets from arguments by default.

## Notable Quotes

> "not to be a hard security boundary"

## Takeaways

- SafeExec is a practical local-shell guardrail for accidental destructive
  commands, especially in AI-agent sessions, but it is not isolation.
- The strongest design choices are real-TTY confirmation, one-time challenge
  tokens, foreground-process checks, fail-closed behavior, and explicit bypasses.
- A serious shell-execution story still needs harness permissions, typed tools,
  project-specific policy, auditing, and sandboxing for untrusted execution.
