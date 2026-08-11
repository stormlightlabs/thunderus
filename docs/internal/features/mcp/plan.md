# MCP

MCP provides typed operations, resources, prompts, discovery, and server
lifecycle. Project trust, authority, containment, redaction, auditing, and
transcript behavior remain shared application policy. Discovering project MCP
configuration never activates it, and server startup or capability use cannot
exceed the current run's authority.

Resources provide structured context without representing every read as a tool
call or injecting server content at startup. Listing stays compact and
namespaced; fetching is explicit and bounded. Every configured server exposes
the same lifecycle vocabulary so failures identify their configuration scope
and phase without exposing secrets.
