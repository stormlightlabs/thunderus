// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://thndrs.stormlightlabs.org",
  integrations: [
    starlight({
      title: "thndrs",
      description:
        "thndrs is a Rust coding harness with a terminal-first workflow, visible context, \
        and bounded repository tools.",
      social: [
        { icon: "github", label: "GitHub", href: "https://github.com/stormlightlabs/thndrs" },
        { icon: "blueSky", label: "BlueSky", href: "https://bsky.app/profile/stormlightlabs.org" },
      ],
      customCss: [
        "@fontsource-variable/ibm-plex-sans",
        "@fontsource-variable/literata",
        "./src/styles/theme.css",
      ],
      head: [
        {
          tag: "meta",
          attrs: {
            property: "og:description",
            content:
              "Use coding models from a terminal UI with visible context,\
              structured transcript events, and bounded repository tools.",
          },
        },
        { tag: "meta", attrs: { property: "og:type", content: "website" } },
        { tag: "meta", attrs: { property: "og:site_name", content: "thndrs" } },
        { tag: "meta", attrs: { name: "twitter:card", content: "summary" } },
      ],
      sidebar: [
        /* Getting Started: short orientation and first-run material for readers who are new to thndrs. */
        {
          label: "Getting Started",
          items: [
            { label: "Introduction", slug: "introduction" },
            { label: "Installation", slug: "getting-started/installation" },
            { label: "Quick Start", slug: "getting-started/quick-start" },
            { label: "Manual", slug: "getting-started/cli-usage" },
          ],
        },
        /* Usage: task-oriented pages for operating thndrs. */
        {
          label: "Usage",
          items: [
            { label: "Interaction", slug: "usage/prompting-and-input" },
            { label: "Keybindings", slug: "usage/keybinds" },
            { label: "Project Context", slug: "usage/project-context" },
            { label: "Skills", slug: "usage/skills" },
            { label: "Tools", slug: "usage/tools" },
            { label: "Web Search", slug: "usage/web-search" },
            { label: "Models", slug: "usage/models" },
            { label: "Sessions", slug: "usage/sessions" },
            { label: "Security", slug: "usage/security-and-permissions" },
          ],
        },
        /* Concepts: stable mental models that explain behavior. */
        {
          label: "Concepts",
          items: [
            { label: "Prompt Assembly", slug: "concepts/prompt-assembly" },
            { label: "Prompt XML", slug: "concepts/prompt-xml-syntax" },
            { label: "Transcripts", slug: "concepts/transcript-model" },
            { label: "Tools", slug: "concepts/tool-boundary" },
            { label: "TUI", slug: "usage/tui" },
          ],
        },
        /* Providers: integration-specific behavior and provider boundaries. */
        {
          label: "Providers",
          items: [
            { label: "Umans", slug: "providers/umans" },
            { label: "OpenCode Go", slug: "providers/opencode-go" },
            { label: "Search and Extraction", slug: "providers/search-and-extraction" },
          ],
        },
        /* Reference: exact contracts, schemas, commands, and configuration surfaces. */
        {
          label: "Reference",
          collapsed: true,
          items: [
            { label: "CLI Reference", slug: "reference/cli" },
            { label: "Configuration", slug: "reference/configuration" },
            { label: "Environment Variables", slug: "reference/environment-variables" },
            { label: "Tool Reference", slug: "reference/tools" },
            { label: "Session Format", slug: "reference/session-format" },
          ],
        },
        /* Development: contributor-facing internals, workflows, invariants, and test strategy */
        {
          label: "Development",
          collapsed: true,
          items: [
            { label: "Architecture", slug: "development/architecture" },
            { label: "Workflow", slug: "development/workflow" },
            { label: "Testing", slug: "development/testing" },
          ],
        },
        /* Notebook: research notes and working synthesis */
        {
          label: "Notebook",
          collapsed: true,
          items: [
            {
              label: "Agent Harnesses",
              collapsed: true,
              items: [
                { label: "Harness Engineering", slug: "notebook/harness-engineering" },
                { label: "Pi", slug: "notebook/pi" },
                { label: "Herdr (multiplexer)", slug: "notebook/herdr" },
              ],
            },
            {
              label: "Prompts",
              collapsed: true,
              items: [
                { label: "Prompting", slug: "notebook/prompts" },
                { label: "Prompting with Codex", slug: "notebook/codex-prompting-guide" },
                {
                  label: "System Prompts",
                  items: [
                    { label: "Claude Code", slug: "notebook/claude-system-prompts" },
                    { label: "Goose", slug: "notebook/goose-prompts" },
                    { label: "Codex", slug: "notebook/codex-prompts" },
                    { label: "Pi", slug: "notebook/pi-prompts" },
                  ],
                },
              ],
            },
            {
              label: "Context",
              collapsed: true,
              items: [
                { label: "Context Control", slug: "notebook/context-control" },
                { label: "AGENTS.md", slug: "notebook/agents-md" },
                { label: "SKILLS.md", slug: "notebook/skills" },
              ],
            },
            {
              label: "Tools and Providers",
              collapsed: true,
              items: [
                {
                  label: "Filesystem Traversal",
                  slug: "notebook/fs-traversal",
                },
                { label: "Umans.ai", slug: "notebook/providers/umans" },
                { label: "OpenCode Go", slug: "notebook/providers/opencode-go" },
              ],
            },
            {
              label: "UI and Layout",
              collapsed: true,
              items: [
                { label: "Ratatui Patterns", slug: "notebook/ratatui" },
                { label: "insta Snapshot Testing", slug: "notebook/ratatui-testing" },
                { label: "Text Input Libs", slug: "notebook/text-input-libraries" },
                {
                  label: "Prompt Libs",
                  slug: "notebook/prompt-renderer-research",
                },
                { label: "Terminal Agent UI", slug: "notebook/ui" },
                { label: "Yoga Layout Engine", slug: "notebook/yoga" },
                { label: "Yoga for Gridland", slug: "notebook/yoga-gridland" },
                { label: "Gridland UI", slug: "notebook/ui-patterns" },
              ],
            },
            {
              label: "Meta",
              collapsed: true,
              items: [
                { label: "Docs", slug: "notebook/docs" },
                { label: "Sessions", slug: "notebook/sessions" },
                {
                  label: "Releasing",
                  slug: "notebook/release",
                },
                { label: "Memory (Letta)", slug: "notebook/letta" },
              ],
            },
          ],
        },
      ],
    }),
  ],
});
