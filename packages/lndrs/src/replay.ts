import type { FrontendEvent, FrontendSnapshot } from "./protocol/messages.ts";

export const REPLAY_SCHEMA_VERSION = "thndrs-frontend-replay-v1" as const;

export type ReplayStep = { delay_ms: number; event: FrontendEvent };

export type ReplayFixture = {
  schema_version: typeof REPLAY_SCHEMA_VERSION;
  name: string;
  terminal: { width: number; height: number };
  snapshot: FrontendSnapshot;
  steps: ReplayStep[];
};

export type ReplayTiming = "immediate" | "timed";

export async function loadReplayFixture(path: string): Promise<ReplayFixture> {
  const value: unknown = await Bun.file(path).json();
  return parseReplayFixture(value);
}

export function parseReplayFixture(value: unknown): ReplayFixture {
  if (!isRecord(value) || value.schema_version !== REPLAY_SCHEMA_VERSION) {
    throw new Error(`replay fixture must use schema_version ${REPLAY_SCHEMA_VERSION}`);
  }
  if (typeof value.name !== "string" || value.name.length === 0) throw new Error("replay fixture must have a name");
  if (
    !isRecord(value.terminal) ||
    !isPositiveInteger(value.terminal.width) ||
    !isPositiveInteger(value.terminal.height)
  ) {
    throw new Error("replay fixture terminal dimensions must be positive integers");
  }
  if (!isSnapshot(value.snapshot)) throw new Error("replay fixture snapshot is malformed");
  if (!Array.isArray(value.steps)) throw new Error("replay fixture steps must be an array");
  for (const [index, step] of value.steps.entries()) {
    if (!isRecord(step) || !Number.isFinite(step.delay_ms) || (step.delay_ms as number) < 0 || !isEvent(step.event)) {
      throw new Error(`replay fixture step ${index} is malformed`);
    }
  }
  return value as unknown as ReplayFixture;
}

export async function playReplay(
  fixture: ReplayFixture,
  apply: (event: FrontendEvent) => void,
  timing: ReplayTiming = "immediate",
  sleep: (milliseconds: number) => Promise<void> = Bun.sleep,
): Promise<void> {
  for (const step of fixture.steps) {
    if (timing === "timed" && step.delay_ms > 0) await sleep(step.delay_ms);
    apply(step.event);
  }
}

function isSnapshot(value: unknown): value is FrontendSnapshot {
  return (
    isRecord(value) &&
    isRecord(value.session) &&
    typeof value.workspace === "string" &&
    typeof value.model === "string" &&
    typeof value.reasoning_effort === "string" &&
    isRecord(value.run) &&
    Array.isArray(value.transcript) &&
    Array.isArray(value.queue) &&
    isRecord(value.usage) &&
    typeof value.truncated === "boolean" &&
    typeof value.event_sequence === "number"
  );
}

const replayEventTypes = new Set<FrontendEvent["type"]>([
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
  "snapshot.updated",
]);

function isEvent(value: unknown): value is FrontendEvent {
  return isRecord(value) && typeof value.type === "string" && replayEventTypes.has(value.type as FrontendEvent["type"]);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isPositiveInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0;
}
