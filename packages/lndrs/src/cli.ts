import type { LandorusOptions } from "./app.ts";

export function parseArguments(arguments_: string[]): LandorusOptions {
  const options: LandorusOptions = {};
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    switch (argument) {
      case "--replay":
        options.replayPath = requiredValue(arguments_, ++index, argument);
        break;
      case "--replay-timing": {
        const timing = requiredValue(arguments_, ++index, argument);
        if (timing !== "immediate" && timing !== "timed") {
          throw new Error("--replay-timing must be immediate or timed");
        }
        options.replayTiming = timing;
        break;
      }
      case "--width":
        options.width = positiveInteger(requiredValue(arguments_, ++index, argument), argument);
        break;
      case "--height":
        options.height = positiveInteger(requiredValue(arguments_, ++index, argument), argument);
        break;
      default:
        throw new Error(`unsupported argument ${argument}`);
    }
  }
  if (options.replayTiming && !options.replayPath) throw new Error("--replay-timing requires --replay");
  return options;
}

function requiredValue(arguments_: string[], index: number, option: string): string {
  const value = arguments_[index];
  if (!value || value.startsWith("--")) throw new Error(`${option} requires a value`);
  return value;
}

function positiveInteger(value: string, option: string): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) throw new Error(`${option} must be a positive integer`);
  return parsed;
}
