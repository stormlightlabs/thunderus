---
# https://vitepress.dev/reference/default-theme-home-page
layout: home

hero:
  name: "Thunderus"
  text: "Harness-first coding agent"
  tagline: "A Rust-native TUI for safe, reviewable, shell-first automation."
  actions:
    - theme: brand
      text: Getting Started
      link: /getting-started
    - theme: alt
      text: Philosophy
      link: /guide/philosophy
    - theme: alt
      text: Configuration
      link: /reference/configuration

features:
  - title: Harness-First Workflow
    details: A TUI workbench designed for planning, review, and controlled execution.
  - title: Approval-Centric Safety
    details: Every tool and shell command is gated by explicit approvals and sandbox policies.
  - title: Diff-First Editing
    details: All changes are presented as reviewable diffs before they land in your repo.
  - title: Mixed-Initiative Collaboration
    details: The agent pauses when you type, then reconciles your edits before continuing.
  - title: Observability by Design
    details: Trajectory and inspector views make "why did it do that?" a first-class question.
  - title: Extensible by Skills
    details: On-demand skill loading today; plugin runtime support is planned.
---
