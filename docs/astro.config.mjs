// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import UnoCSS from "unocss/astro";
import starlinkValidator from "starlight-links-validator";
import starLlmsTxt from "starlight-llms-txt";
import ogImages from "./src/integrations/og-images/index.ts";

export default defineConfig({
  site: "https://thndrs.stormlightlabs.org",
  integrations: [
    UnoCSS(),
    ogImages(),
    starlight({
      plugins: [starlinkValidator(), starLlmsTxt()],
      title: "thndrs",
      description:
        "thndrs is a Rust coding harness with a terminal-first workflow, visible context, and bounded repository tools.",
      social: [
        { icon: "github", label: "GitHub", href: "https://github.com/stormlightlabs/thunderus" },
        { icon: "blueSky", label: "BlueSky", href: "https://bsky.app/profile/stormlightlabs.org" },
      ],
      customCss: ["@fontsource-variable/ibm-plex-sans", "@fontsource-variable/literata", "./src/styles/theme.css"],
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
        { tag: "meta", attrs: { property: "og:image", content: "https://thndrs.stormlightlabs.org/og.png" } },
        { tag: "meta", attrs: { property: "og:image:width", content: "1200" } },
        { tag: "meta", attrs: { property: "og:image:height", content: "630" } },
        { tag: "meta", attrs: { property: "og:image:type", content: "image/png" } },
        { tag: "meta", attrs: { name: "twitter:card", content: "summary_large_image" } },
        { tag: "meta", attrs: { name: "twitter:image", content: "https://thndrs.stormlightlabs.org/og.png" } },
      ],
      sidebar: [
        /* Getting Started: short orientation and first-run material for readers who are new to thndrs. */
        {
          label: "Getting Started",
          items: [
            { label: "Introduction", slug: "docs" },
            { label: "Installation", slug: "docs/getting-started/installation" },
            { label: "Quick Start", slug: "docs/getting-started/quick-start" },
            { label: "Manual", slug: "docs/getting-started/cli-usage" },
          ],
        },
        /* Usage: task-oriented pages for operating thndrs. */
        {
          label: "Usage",
          items: [
            { label: "Interaction", slug: "docs/usage/prompting-and-input" },
            { label: "Prompt Templates", slug: "docs/usage/prompt-templates" },
            { label: "Keybindings", slug: "docs/usage/keybinds" },
            { label: "Project Context", slug: "docs/usage/project-context" },
            { label: "Skills", slug: "docs/usage/skills" },
            { label: "Tools", slug: "docs/usage/tools" },
            { label: "MCP", slug: "docs/usage/mcp" },
            { label: "ACP", slug: "docs/usage/acp" },
            { label: "Web Search", slug: "docs/usage/web-search" },
            { label: "Models", slug: "docs/usage/models" },
            { label: "Sessions", slug: "docs/usage/sessions" },
            { label: "Security", slug: "docs/usage/security-and-permissions" },
          ],
        },
        /* Concepts: stable mental models that explain behavior. */
        {
          label: "Concepts",
          collapsed: true,
          items: [
            {
              label: "Prompts",
              items: [
                { label: "Prompt Assembly", slug: "docs/concepts/prompt-assembly" },
                { label: "Prompt XML", slug: "docs/concepts/prompt-xml-syntax" },
              ],
              collapsed: true,
            },
            { label: "Transcripts", slug: "docs/concepts/transcript-model" },
            { label: "Context Compaction", slug: "docs/concepts/context-compaction" },
          ],
        },
        /* Providers: integration-specific behavior and provider boundaries. */
        {
          label: "Providers",
          collapsed: true,
          items: [
            { label: "OpenCode Go", slug: "docs/providers/opencode-go" },
            { label: "OpenCode Zen", slug: "docs/providers/opencode-zen" },
            { label: "ChatGPT Codex", slug: "docs/providers/chatgpt" },
          ],
        },
        /* Reference: exact contracts, schemas, commands, and configuration surfaces. */
        {
          label: "Reference",
          items: [
            { label: "CLI Reference", slug: "docs/reference/cli" },
            { label: "Configuration", slug: "docs/reference/configuration" },
            { label: "Environment Variables", slug: "docs/reference/environment-variables" },
            { label: "Tool Reference", slug: "docs/reference/tools" },
            { label: "Session Format", slug: "docs/reference/session-format" },
          ],
        },
        /* Development: contributor-facing internals, workflows, invariants, and test strategy */
        {
          label: "Development",
          collapsed: true,
          items: [
            { label: "Architecture", slug: "docs/development/architecture" },
            { label: "Workflow", slug: "docs/development/workflow" },
            { label: "Testing", slug: "docs/development/testing" },
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
                { label: "Harness Engineering", slug: "docs/notebook/harness-engineering" },
                { label: "Pi", slug: "docs/notebook/pi" },
                { label: "Tau", slug: "docs/notebook/tau" },
                { label: "Herdr (multiplexer)", slug: "docs/notebook/herdr" },
              ],
            },
            {
              label: "Prompts",
              collapsed: true,
              items: [
                { label: "Prompting", slug: "docs/notebook/prompts" },
                { label: "Prompting with Codex", slug: "docs/notebook/codex-prompting-guide" },
                {
                  label: "System Prompts",
                  items: [
                    { label: "Claude Code", slug: "docs/notebook/claude-system-prompts" },
                    { label: "Goose", slug: "docs/notebook/goose-prompts" },
                    { label: "Codex", slug: "docs/notebook/codex-prompts" },
                    { label: "Pi", slug: "docs/notebook/pi-prompts" },
                  ],
                },
              ],
            },
            {
              label: "Context",
              collapsed: true,
              items: [
                { label: "Context Control", slug: "docs/notebook/context-control" },
                { label: "Context Observability", slug: "docs/notebook/context-observability" },
                { label: "Token Optimization", slug: "docs/notebook/token-optimization" },
                { label: "AGENTS.md", slug: "docs/notebook/agents-md" },
                { label: "SKILLS.md", slug: "docs/notebook/skills" },
                { label: "MCP", slug: "docs/notebook/mcp" },
              ],
            },
            {
              label: "Tools and Providers",
              collapsed: true,
              items: [
                { label: "Filesystem Traversal", slug: "docs/notebook/fs-traversal" },
                { label: "Shell Execution", slug: "docs/notebook/shellexec" },
                { label: "Protocols", slug: "docs/notebook/providers/protocols" },
                { label: "Umans.ai", slug: "docs/notebook/providers/umans" },
                { label: "OpenCode Go", slug: "docs/notebook/providers/opencode-go" },
                { label: "OpenCode Zen", slug: "docs/notebook/providers/opencode-zen" },
                { label: "ChatGPT Codex", slug: "docs/notebook/providers/chatgpt" },
              ],
            },
            {
              label: "UI and Layout",
              collapsed: true,
              items: [
                { label: "Ratatui Patterns", slug: "docs/notebook/ratatui" },
                { label: "insta Snapshot Testing", slug: "docs/notebook/ratatui-testing" },
                { label: "Text Input Libs", slug: "docs/notebook/text-input-libraries" },
                { label: "Prompt Libs", slug: "docs/notebook/prompt-renderer-research" },
                { label: "Terminal Agent UI", slug: "docs/notebook/ui" },
                { label: "Time to First Token", slug: "docs/notebook/ttft" },
                { label: "Yoga Layout Engine", slug: "docs/notebook/yoga" },
                { label: "Yoga for Gridland", slug: "docs/notebook/yoga-gridland" },
                { label: "Gridland UI", slug: "docs/notebook/ui-patterns" },
                { label: "iocraft", slug: "docs/notebook/iocraft" },
              ],
            },
            {
              label: "Meta",
              collapsed: true,
              items: [
                { label: "New Codebases", slug: "docs/notebook/new-codebase" },
                { label: "Docs", slug: "docs/notebook/docs" },
                { label: "Sessions", slug: "docs/notebook/sessions" },
                { label: "Releasing", slug: "docs/notebook/release" },
                { label: "Memory (Letta)", slug: "docs/notebook/letta" },
                { label: "Memory Retrieval", slug: "docs/notebook/memory-retrieval" },
              ],
            },
          ],
        },
      ],
    }),
  ],
});
