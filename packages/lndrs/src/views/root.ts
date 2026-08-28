import { BoxRenderable, type CliRenderer, type RenderContext, TextRenderable } from "@opentui/core";

export interface RootView {
  root: BoxRenderable;
  transcript: TextRenderable;
  status: TextRenderable;
}

export function createRootView(context: RenderContext): RootView {
  const root = new BoxRenderable(context, { id: "lndrs-root", width: "100%", height: "100%", flexDirection: "column" });
  const transcript = new TextRenderable(context, {
    id: "transcript",
    content: "Landorus\n\nConnecting to thndrs…",
    flexGrow: 1,
    padding: 1,
  });
  const composer = new BoxRenderable(context, {
    id: "composer",
    height: 3,
    border: true,
    borderStyle: "single",
    borderColor: "#666666",
    paddingX: 1,
  });
  composer.add(new TextRenderable(context, { content: "Composer arrives in LNDRS-4", fg: "#777777" }));
  const status = new TextRenderable(context, {
    id: "status",
    content: "no model · 0 tokens · Connecting · q quit",
    height: 1,
    fg: "#888888",
  });

  root.add(transcript);
  root.add(composer);
  root.add(status);
  return { root, transcript, status };
}

export function mountRootView(renderer: CliRenderer): RootView {
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  return view;
}
