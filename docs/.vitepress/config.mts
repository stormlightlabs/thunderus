import { defineConfig } from "vitepress";

// https://vitepress.dev/reference/site-config
export default defineConfig({
    title: "Thunderus",
    description: "A harness-first coding agent built in Rust",
    themeConfig: {
        // https://vitepress.dev/reference/default-theme-config
        nav: [
            { text: "Guide", link: "/guide/" },
            { text: "Concepts", link: "/concepts/" },
            { text: "Development", link: "/development/" },
            { text: "Reference", link: "/reference/" },
        ],
        sidebar: [
            {
                text: "Getting Started",
                items: [{ text: "Introduction", link: "/getting-started" }],
            },
            {
                text: "Guide",
                items: [
                    { text: "Overview", link: "/guide/" },
                    { text: "Philosophy", link: "/guide/philosophy" },
                    { text: "TUI Workbench", link: "/guide/tui" },
                    { text: "Workflows", link: "/guide/workflows" },
                ],
            },
            {
                text: "Concepts",
                items: [
                    { text: "Overview", link: "/concepts/" },
                    { text: "System", link: "/concepts/system" },
                    { text: "Architecture", link: "/concepts/architecture" },
                    { text: "Trajectories", link: "/concepts/trajectories" },
                ],
            },
            {
                text: "Development",
                items: [
                    { text: "Overview", link: "/development/" },
                    { text: "Architecture", link: "/development/architecture" },
                    { text: "Data Flow", link: "/development/data-flow" },
                    { text: "Patterns", link: "/development/patterns" },
                    { text: "Workflow", link: "/development/development" },
                ],
            },
            {
                text: "Reference",
                items: [
                    { text: "Overview", link: "/reference/" },
                    { text: "Configuration", link: "/reference/configuration" },
                    { text: "CLI", link: "/reference/cli" },
                    { text: "Keybindings", link: "/reference/keybindings" },
                ],
            },
            {
                text: "Extensibility",
                items: [{ text: "Overview", link: "/extensibility/" }],
            },
            {
                text: "Contributing",
                items: [{ text: "Overview", link: "/contributing/" }],
            },
        ],

        socialLinks: [
            {
                icon: "github",
                link: "https://github.com/stormlightlabs/THUNDERUS",
            },
        ],
    },
});
