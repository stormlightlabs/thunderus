import { runLandorus } from "./app.ts";
import { parseArguments } from "./cli.ts";

try {
  await runLandorus(parseArguments(process.argv.slice(2)));
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
