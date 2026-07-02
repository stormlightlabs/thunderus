// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://thndrs.stormlightlabs.org",
  integrations: [
    starlight({
      title: "thndrs",
      description:
        "thndrs is a Rust coding harness with a terminal-first workflow, visible context, and bounded repository tools.",
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
              "Use coding models from a terminal UI with visible context, structured transcript events, and bounded repository tools.",
          },
        },
        { tag: "meta", attrs: { property: "og:type", content: "website" } },
        { tag: "meta", attrs: { property: "og:site_name", content: "thndrs" } },
        { tag: "meta", attrs: { name: "twitter:card", content: "summary" } },
      ],
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Introduction", slug: "guides" },
            { label: "Installation", slug: "guides/getting-started/installation" },
            { label: "Quick Start", slug: "guides/getting-started/quick-start" },
            { label: "Manual", slug: "guides/getting-started/cli-usage" },
          ],
        },
        {
          label: "Usage",
          items: [
            { label: "Interaction", slug: "guides/usage/prompting-and-input" },
            { label: "Keybindings", slug: "guides/keybinds" },
            { label: "Project Context", slug: "guides/usage/project-context" },
            { label: "Skills", slug: "guides/usage/skills" },
            { label: "Tools", slug: "guides/usage/tools" },
            { label: "Web Search", slug: "guides/usage/web-search" },
            { label: "Models", slug: "guides/usage/models" },
            { label: "Sessions", slug: "guides/usage/sessions" },
            { label: "Security", slug: "guides/usage/security-and-permissions" },
          ],
        },
        {
          label: "Concepts",
          items: [
            { label: "Prompt Assembly", slug: "guides/concepts/prompt-assembly" },
            { label: "Prompt XML", slug: "guides/concepts/prompt-xml-syntax" },
            { label: "Transcripts", slug: "guides/concepts/transcript-model" },
            { label: "Tools", slug: "guides/concepts/tool-boundary" },
            { label: "TUI", slug: "guides/usage/tui" },
          ],
        },
        {
          label: "Providers",
          items: [
            { label: "Umans", slug: "guides/providers/umans" },
            { label: "OpenCode Go", slug: "guides/providers/opencode-go" },
            { label: "Search and Extraction", slug: "guides/providers/search-and-extraction" },
          ],
        },
        {
          label: "Reference",
          collapsed: true,
          items: [
            { label: "CLI Reference", slug: "guides/reference/cli" },
            { label: "Configuration", slug: "guides/reference/configuration" },
            { label: "Environment Variables", slug: "guides/reference/environment-variables" },
            { label: "Tool Reference", slug: "guides/reference/tools" },
            { label: "Session Format", slug: "guides/reference/session-format" },
          ],
        },
        {
          label: "Development",
          collapsed: true,
          items: [
            { label: "Architecture", slug: "guides/development/architecture" },
            { label: "Workflow", slug: "guides/development/workflow" },
            { label: "Testing", slug: "guides/development/testing" },
          ],
        },
        {
          label: "Notebook",
          collapsed: true,
          items: [
            {
              label: "Agent Harnesses",
              collapsed: true,
              items: [
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
                { label: "Yoga Layout Engine", slug: "notebook/yoga" },
                { label: "Yoga for Gridland", slug: "notebook/yoga-gridland" },
                { label: "Gridland UI", slug: "notebook/ui-patterns" },
              ],
            },
            {
              label: "Meta",
              collapsed: true,
              items: [
                { label: "Sessions", slug: "notebook/sessions" },
                {
                  label: "Releasing",
                  slug: "notebook/release",
                },
              ],
            },
          ],
        },
      ],
    }),
  ],
});
