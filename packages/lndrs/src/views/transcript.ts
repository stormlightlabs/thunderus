import {
  BoxRenderable,
  ScrollBoxRenderable,
  TextRenderable,
  dim,
  fg,
  t,
  type MouseEvent,
  type RenderContext,
  type StyledText,
} from "@opentui/core";
import type { TranscriptItem } from "../protocol/messages.ts";

const COLORS = {
  assistant: "#67d4e8",
  dim: "#777777",
  error: "#ff6b6b",
  reasoning: "#9b8fc9",
  skill: "#c79bf2",
  success: "#70c991",
  tool: "#d4a15a",
  user: "#f0c674",
} as const;

export interface TranscriptBlockView {
  readonly id: string;
  readonly root: BoxRenderable;
  readonly itemKind: TranscriptItem["kind"];
  update(item: TranscriptItem): void;
  setExpanded(expanded: boolean): void;
}

export class TranscriptView {
  readonly scroll: ScrollBoxRenderable;
  readonly #context: RenderContext;
  readonly #empty: TextRenderable;
  readonly #blocks = new Map<string, TranscriptBlockView>();
  #reconciliationCount = 0;

  constructor(context: RenderContext) {
    this.#context = context;
    this.scroll = new ScrollBoxRenderable(context, {
      id: "transcript",
      flexGrow: 1,
      focusable: true,
      scrollY: true,
      scrollX: false,
      stickyScroll: true,
      stickyStart: "bottom",
      viewportCulling: true,
      contentOptions: { flexDirection: "column", paddingTop: 1, paddingRight: 2, paddingBottom: 1, paddingLeft: 1 },
      verticalScrollbarOptions: {
        showArrows: false,
        trackOptions: { backgroundColor: "#333333", foregroundColor: "#666666" },
      },
    });
    this.#empty = new TextRenderable(context, {
      id: "transcript-empty",
      content: "Landorus\n\nStart a conversation.",
      fg: COLORS.dim,
    });
    this.scroll.add(this.#empty);
  }

  reconcile(items: TranscriptItem[]): void {
    this.#reconciliationCount += 1;
    this.#empty.visible = items.length === 0;
    const present = new Set(items.map((item) => item.id));
    for (const [id, block] of this.#blocks) {
      if (present.has(id)) continue;
      this.scroll.remove(block.root);
      block.root.destroyRecursively();
      this.#blocks.delete(id);
    }

    for (const [index, item] of items.entries()) {
      const existing = this.#blocks.get(item.id);
      if (existing) {
        if (existing.itemKind !== item.kind) {
          throw new Error(`transcript item ${item.id} changed kind`);
        }
        existing.update(item);
        continue;
      }

      const block = createTranscriptBlock(this.#context, item);
      this.#blocks.set(item.id, block);
      this.scroll.add(block.root, index + 1);
    }
  }

  get blockCount(): number {
    return this.#blocks.size;
  }

  get reconciliationCount(): number {
    return this.#reconciliationCount;
  }

  getBlock(id: string): TranscriptBlockView | undefined {
    return this.#blocks.get(id);
  }

  setToolExpanded(id: string, expanded: boolean): void {
    this.#blocks.get(id)?.setExpanded(expanded);
  }
}

function createTranscriptBlock(context: RenderContext, item: TranscriptItem): TranscriptBlockView {
  const root = new BoxRenderable(context, { id: item.id, width: "100%", flexDirection: "column", marginBottom: 1 });
  const primary = new TextRenderable(context, { id: `${item.id}-content`, content: "", width: "100%" });
  root.add(primary);

  let signature = "";
  let expanded = false;
  let currentItem = item;

  const render = () => {
    const nextSignature = blockSignature(currentItem, expanded);
    if (nextSignature === signature) return;
    signature = nextSignature;
    primary.content = presentBlock(currentItem, expanded);
  };

  const block: TranscriptBlockView = {
    id: item.id,
    root,
    itemKind: item.kind,
    update(next) {
      currentItem = next;
      render();
    },
    setExpanded(next) {
      if (currentItem.kind !== "tool") return;
      expanded = next;
      render();
    },
  };

  if (item.kind === "tool") {
    const toggleExpanded = (event: MouseEvent) => {
      if (event.button !== 0) return;
      expanded = !expanded;
      render();
      event.stopPropagation();
    };
    root.onMouseUp = toggleExpanded;
    primary.onMouseUp = toggleExpanded;
  }

  render();
  return block;
}

function blockSignature(item: TranscriptItem, expanded: boolean): string {
  switch (item.kind) {
    case "user":
    case "status":
    case "error":
      return `${item.kind}\u0000${item.text}`;
    case "assistant":
    case "reasoning":
      return `${item.kind}\u0000${item.streaming}\u0000${item.text}`;
    case "skill":
      return `skill\u0000${item.name}\u0000${item.path}`;
    case "tool":
      return `tool\u0000${expanded}\u0000${item.name}\u0000${item.arguments}\u0000${item.status}\u0000${item.output.join("\u0000")}`;
  }
}

function presentBlock(item: TranscriptItem, expanded: boolean): StyledText | string {
  switch (item.kind) {
    case "user":
      return t`${fg(COLORS.user)("you")}\n${item.text}`;
    case "assistant":
      return t`${fg(COLORS.assistant)("landorus")}\n${item.text}`;
    case "reasoning":
      return t`${fg(COLORS.reasoning)("reasoning")}\n${dim(item.text)}`;
    case "skill":
      return t`${fg(COLORS.skill)(`◇ skill  ${item.name}`)}\n${dim(`  ${item.path}`)}`;
    case "status":
      return t`${dim(`· ${item.text}`)}`;
    case "error":
      return t`${fg(COLORS.error)("!")} ${item.text}`;
    case "tool":
      return presentTool(item, expanded);
  }
}

function presentTool(item: Extract<TranscriptItem, { kind: "tool" }>, expanded: boolean): StyledText {
  const state = toolState(item.status);
  const target = summarizeArguments(item.arguments);
  const head = `${expanded ? "⌄" : "›"} ${state.icon} ${item.name}`;
  if (!expanded) return t`${fg(state.color)(head)}${target ? `  ${target}` : ""}`;

  const outputHeading = item.output.length > 0 ? dim("\n\noutput") : "";
  const output = item.output.length > 0 ? `\n${item.output.join("\n")}` : "";
  return t`${fg(state.color)(head)}${target ? `  ${target}` : ""}\n\n${dim("arguments")}\n${item.arguments || "(none)"}${outputHeading}${output}`;
}

function toolState(status: string): { icon: string; color: string } {
  switch (status) {
    case "ok":
      return { icon: "✓", color: COLORS.success };
    case "failed":
      return { icon: "✕", color: COLORS.error };
    case "cancelled":
      return { icon: "■", color: COLORS.dim };
    default:
      return { icon: "◐", color: COLORS.tool };
  }
}

function summarizeArguments(argumentsText: string): string {
  if (!argumentsText) return "";
  try {
    const value = JSON.parse(argumentsText) as unknown;
    if (typeof value === "object" && value !== null && !Array.isArray(value)) {
      const record = value as Record<string, unknown>;
      for (const key of ["path", "file_path", "command", "query", "pattern"]) {
        if (typeof record[key] === "string") return compact(record[key]);
      }
    }
  } catch {
    // Non-JSON tool arguments still make a useful compact target.
  }
  return compact(argumentsText.replaceAll("\n", " "));
}

function compact(value: string): string {
  const limit = 72;
  return value.length <= limit ? value : `${value.slice(0, limit - 1)}…`;
}
