---
title: "Web search and URL reading"
---

`thndrs` discovers pages through configured MCP servers. It does not include a
web-search backend or a search tool by default. If no configured server exposes
a search tool, the agent can read a URL it already has but cannot discover new
pages.

## Configure a search server

Add an MCP server with a search capability to your global or project MCP
configuration, then review and trust project configuration before using it. The
server chooses its own tool names and authentication. `thndrs` namespaces the
tools it discovers and sends them to the model with the rest of the tool
catalog.

[xngmcp](https://github.com/stormlightlabs/xngmcp), backed by a local SearXNG
instance, is one option. You can use any MCP server that provides the search
service you need; `thndrs` does not require xngmcp or any particular package or
tool name.

See [MCP](/docs/usage/mcp/) for configuration, trust, and diagnostics.

## Read returned URLs

`read_url` remains built in. Use it for a public URL supplied by the user, found
in the workspace, or returned from an MCP search result. It accepts only HTTP
and HTTPS URLs that resolve to public addresses. Each redirect is checked, and
response size, redirect count, and total request time are capped.

HTML is extracted to readable Markdown with Lectito. Plain text, JSON, XML,
feeds, YAML, CSV, and JavaScript are returned as text. Binary content is
rejected. The result records its final URL and retrieval diagnostics.
