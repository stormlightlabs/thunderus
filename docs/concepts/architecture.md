---
outline: deep
---

# Core Concepts

This section explains the core building blocks that make Thunderus predictable
and safe.

## Approval System

Every tool execution flows through an approval gate. Approval mode and sandbox policy
determine whether an action is permitted, requires confirmation, or is blocked.

## Diff-First Editing

Edits are collected into patches and presented for review before they are applied.
This keeps changes reversible and makes it easy to reason about impact.

## Event Logs

Sessions are recorded as event streams. The intent is to make reasoning and
state transitions explicit, enabling future replay, inspection, and auditing.

## Memory Layers

Thunderus uses tiered memory (core, semantic, procedural, episodic) with a
gardener process that consolidates session history into durable artifacts.
