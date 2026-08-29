import type { SelectOption } from "@opentui/core";
import type { AppState } from "../state/app.svelte.ts";
import type { RootView } from "./root.ts";

export function bindRootView(state: AppState, view: RootView): () => void {
  return $effect.root(() => {
    $effect(() => {
      view.transcript.reconcile(state.transcript);
    });
    $effect(() => {
      view.status.content = state.statusText;
      view.composer.root.borderColor =
        state.run.state === "working"
          ? "#c8b67c"
          : state.run.state === "stopping"
            ? "#c48f8f"
            : state.run.state === "error"
              ? "#c48f8f"
              : "#596168";
    });
    $effect(() => {
      const overlay = state.overlay;
      if (!overlay) {
        view.overlay.hide();
        view.composer.input.focus();
        return;
      }
      if (overlay.kind === "context") {
        const context = state.snapshot?.context;
        if (!context) return;
        const percent = Math.round((context.used_tokens * 100) / Math.max(context.context_window, 1));
        view.overlay.showDetail(
          "CONTEXT",
          [
            `used                 ${context.used_tokens} (${percent}%)`,
            `context window       ${context.context_window}`,
            `available input      ${context.available_input}`,
            `selection target     ${context.target_tokens}`,
            `compact at           ${context.auto_compaction_threshold}`,
            `compaction           ${context.compaction_state}`,
            `limits               ${context.limit_source}`,
            "",
            "Esc  return to Stream",
          ].join("\n"),
        );
        return;
      }
      const options = overlayOptions(state, overlay.kind).filter((option) =>
        `${option.name} ${option.description}`.toLowerCase().includes(overlay.query.trim().toLowerCase()),
      );
      const selected = overlay.kind === "permission" ? (state.pendingPermission?.selected ?? 0) : 0;
      view.overlay.showList(overlayTitle(overlay.kind), options, overlay.kind === "palette", selected);
    });
  });
}

function overlayTitle(kind: NonNullable<AppState["overlay"]>["kind"]): string {
  switch (kind) {
    case "palette":
      return "COMMAND PALETTE";
    case "permission":
      return "PERMISSION REQUIRED";
    case "model":
      return "SELECT MODEL";
    case "reasoning":
      return "REASONING EFFORT";
    case "context":
      return "CONTEXT";
  }
}

function overlayOptions(state: AppState, kind: NonNullable<AppState["overlay"]>["kind"]): SelectOption[] {
  switch (kind) {
    case "palette": {
      const options: SelectOption[] = [];
      if (state.snapshot?.context) {
        options.push({
          name: "Context: inspect usage",
          description: "Current window and compaction state",
          value: "context",
        });
      }
      if (state.supports("model.select") && state.capabilities.models.length > 0) {
        options.push({ name: "Model: select model", description: state.snapshot?.model ?? "", value: "model" });
      }
      if (state.supports("reasoning.select") && state.capabilities.reasoning_efforts.length > 1) {
        options.push({
          name: "Reasoning: select effort",
          description: state.snapshot?.reasoning_effort ?? "",
          value: "reasoning",
        });
      }
      options.push(
        { name: "Transcript: jump to bottom", description: "End", value: "bottom" },
        { name: "App: quit", description: "Ctrl+D", value: "quit" },
      );
      return options;
    }
    case "permission":
      return (state.pendingPermission?.options ?? []).map((option) => ({
        name: option.name,
        description: option.kind,
        value: option.id,
      }));
    case "model":
      return state.capabilities.models.map((option) => ({
        name: option.label,
        description: option.detail,
        value: option.label,
      }));
    case "reasoning":
      return state.capabilities.reasoning_efforts.map((option) => ({
        name: option.label,
        description: option.description,
        value: option.value,
      }));
    case "context":
      return [];
  }
}
