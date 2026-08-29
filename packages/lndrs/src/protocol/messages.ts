export const PROTOCOL_VERSION = 1 as const;

export type RunState =
  { state: "idle" } | { state: "working" } | { state: "stopping" } | { state: "error"; message: string };

export type TranscriptItem =
  | { kind: "user"; id: string; text: string }
  | { kind: "assistant"; id: string; text: string; streaming: boolean }
  | { kind: "reasoning"; id: string; text: string; streaming: boolean }
  | { kind: "tool"; id: string; name: string; arguments: string; status: string; output: string[] }
  | { kind: "skill"; id: string; name: string; path: string }
  | { kind: "status"; id: string; text: string }
  | { kind: "error"; id: string; text: string };

export interface ContextSummary {
  used_tokens: number;
  context_window: number;
  available_input: number;
  target_tokens: number;
  auto_compaction_threshold: number;
  compaction_state: string;
  limit_source: string;
}

export interface PermissionOption {
  id: string;
  name: string;
  kind: string;
}

export interface PendingPermission {
  tool_call_id: string;
  title: string;
  selected: number;
  options: PermissionOption[];
}

export interface FrontendCapabilities {
  commands: string[];
  models: Array<{ label: string; detail: string }>;
  reasoning_efforts: Array<{ value: string; label: string; description: string }>;
}

export interface FrontendSnapshot {
  event_sequence: number;
  session: { id: string; ephemeral: boolean; turn_count: number };
  workspace: string;
  model: string;
  reasoning_effort: string;
  run: RunState;
  transcript: TranscriptItem[];
  queue: Array<{ id: string; target: string; text: string; settlement: string }>;
  usage: { input_tokens: number; output_tokens: number };
  context?: ContextSummary | null;
  pending_permission?: PendingPermission | null;
  capabilities?: FrontendCapabilities;
  truncated: boolean;
}

export type FrontendEvent =
  | { type: "run.started" }
  | { type: "run.finished" }
  | { type: "run.cancelled" }
  | { type: "run.failed"; message: string }
  | { type: "assistant.delta"; text: string }
  | { type: "reasoning.delta"; text: string }
  | { type: "tool.started"; id: string; name: string; arguments: string }
  | { type: "tool.finished"; id: string; status: string; output: string[] }
  | { type: "usage.updated"; input_tokens: number; output_tokens: number }
  | { type: "context.updated"; context: ContextSummary }
  | {
      type: "permission.requested";
      tool_call_id: string;
      title: string;
      selected: number;
      options: Array<{ id: string; name: string; kind: string }>;
    }
  | { type: "permission.resolved"; tool_call_id: string; outcome: string }
  | { type: "status.updated"; message: string }
  | {
      type: "model.updated";
      model: string;
      options: Array<{ label: string; detail: string }>;
      reasoning_efforts: Array<{ value: string; label: string; description: string }>;
    }
  | { type: "reasoning.updated"; effort: string };

export type ResponseResult =
  | { kind: "initialized"; protocol_version: number; snapshot: FrontendSnapshot }
  | { kind: "snapshot"; snapshot: FrontendSnapshot }
  | { kind: "accepted" }
  | { kind: "shutdown" };

export interface ResponseError {
  code: string;
  message: string;
}

export type ProtocolMessage =
  | { type: "response"; version: number; id: string; ok: boolean; result?: ResponseResult; error?: ResponseError }
  | { type: "event"; version: number; sequence: number; event: FrontendEvent }
  | { type: "protocol_error"; version: number; error: ResponseError };

export type Command =
  | { command: "initialize"; supported_versions: number[] }
  | { command: "state.snapshot" }
  | { command: "turn.submit"; text: string }
  | { command: "turn.cancel" }
  | { command: "permission.respond"; tool_call_id: string; option_id: string | null }
  | { command: "model.select"; model: string }
  | { command: "reasoning.select"; effort: string }
  | { command: "shutdown" };

export type CommandEnvelope = Command & { version: typeof PROTOCOL_VERSION; id: string };

const eventTypes = new Set([
  "run.started",
  "run.finished",
  "run.cancelled",
  "run.failed",
  "assistant.delta",
  "reasoning.delta",
  "tool.started",
  "tool.finished",
  "usage.updated",
  "context.updated",
  "permission.requested",
  "permission.resolved",
  "status.updated",
  "model.updated",
  "reasoning.updated",
]);

export function parseProtocolMessage(value: unknown): ProtocolMessage {
  if (!isRecord(value) || typeof value.type !== "string" || typeof value.version !== "number") {
    throw new ProtocolParseError("protocol message must contain a type and numeric version");
  }
  if (value.version !== PROTOCOL_VERSION) {
    throw new ProtocolParseError(`unsupported protocol version ${value.version}`);
  }

  if (value.type === "response") {
    if (typeof value.id !== "string" || typeof value.ok !== "boolean") {
      throw new ProtocolParseError("response must contain an id and ok flag");
    }
    return value as ProtocolMessage;
  }
  if (value.type === "protocol_error") {
    if (!isResponseError(value.error)) {
      throw new ProtocolParseError("protocol error is malformed");
    }
    return value as ProtocolMessage;
  }
  if (value.type === "event") {
    if (
      typeof value.sequence !== "number" ||
      !isRecord(value.event) ||
      typeof value.event.type !== "string" ||
      !eventTypes.has(value.event.type)
    ) {
      throw new ProtocolParseError("event is malformed or unsupported");
    }
    return value as ProtocolMessage;
  }
  throw new ProtocolParseError(`unsupported protocol message type ${value.type}`);
}

export class ProtocolParseError extends Error {}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isResponseError(value: unknown): value is ResponseError {
  return isRecord(value) && typeof value.code === "string" && typeof value.message === "string";
}
