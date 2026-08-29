import { BoxRenderable, type CliRenderer, type RenderContext, TextareaRenderable, TextRenderable } from "@opentui/core";
import { composerKeyBindings } from "../interaction.ts";
import { OverlayView } from "./overlay.ts";
import { TranscriptView } from "./transcript.ts";

export interface ComposerView {
  root: BoxRenderable;
  input: TextareaRenderable;
}

export interface RootView {
  root: BoxRenderable;
  transcript: TranscriptView;
  composer: ComposerView;
  status: TextRenderable;
  overlay: OverlayView;
}

export function createRootView(context: RenderContext): RootView {
  const root = new BoxRenderable(context, { id: "lndrs-root", width: "100%", height: "100%", flexDirection: "column" });
  const transcript = new TranscriptView(context);
  const composerRoot = new BoxRenderable(context, {
    id: "composer",
    height: 5,
    border: true,
    borderStyle: "rounded",
    borderColor: "#596168",
    flexDirection: "row",
    paddingLeft: 1,
    paddingRight: 1,
  });
  const marker = new TextRenderable(context, { content: "› ", width: 2, fg: "#b7c58b" });
  const input = new TextareaRenderable(context, {
    id: "composer-input",
    flexGrow: 1,
    height: 3,
    placeholder: "Ask Landorus…",
    placeholderColor: "#596168",
    textColor: "#d8dcdf",
    focusedTextColor: "#d8dcdf",
    wrapMode: "word",
    keyBindings: composerKeyBindings,
  });
  composerRoot.add(marker);
  composerRoot.add(input);
  const status = new TextRenderable(context, {
    id: "status",
    content: "no model · 0 tokens · Connecting · Ctrl+D quit",
    height: 1,
    fg: "#7b838a",
  });

  const overlay = new OverlayView(context);
  root.add(transcript.scroll);
  root.add(composerRoot);
  root.add(status);
  root.add(overlay.root);
  return { root, transcript, composer: { root: composerRoot, input }, status, overlay };
}

export function mountRootView(renderer: CliRenderer): RootView {
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  view.composer.input.focus();
  return view;
}
