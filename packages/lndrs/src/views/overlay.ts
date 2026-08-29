import {
  BoxRenderable,
  InputRenderable,
  SelectRenderable,
  TextRenderable,
  type RenderContext,
  type SelectOption,
} from "@opentui/core";

export class OverlayView {
  readonly root: BoxRenderable;
  readonly panel: BoxRenderable;
  readonly title: TextRenderable;
  readonly search: InputRenderable;
  readonly detail: TextRenderable;
  readonly list: SelectRenderable;

  constructor(context: RenderContext) {
    this.root = new BoxRenderable(context, {
      id: "overlay",
      position: "absolute",
      width: "100%",
      height: "100%",
      zIndex: 100,
      visible: false,
      backgroundColor: "#0b0d0e",
      alignItems: "center",
      justifyContent: "center",
    });
    this.panel = new BoxRenderable(context, {
      id: "overlay-panel",
      width: "80%",
      maxWidth: 72,
      height: 16,
      border: true,
      borderStyle: "rounded",
      borderColor: "#596168",
      backgroundColor: "#111315",
      flexDirection: "column",
      padding: 1,
    });
    this.title = new TextRenderable(context, { id: "overlay-title", height: 1, content: "COMMANDS", fg: "#b7c58b" });
    this.search = new InputRenderable(context, {
      id: "overlay-search",
      width: "100%",
      placeholder: "Type a command…",
      textColor: "#d8dcdf",
      focusedTextColor: "#d8dcdf",
      placeholderColor: "#596168",
    });
    this.detail = new TextRenderable(context, {
      id: "overlay-detail",
      visible: false,
      flexGrow: 1,
      content: "",
      fg: "#d8dcdf",
    });
    this.list = new SelectRenderable(context, {
      id: "overlay-list",
      flexGrow: 1,
      options: [],
      backgroundColor: "#111315",
      focusedBackgroundColor: "#111315",
      textColor: "#d8dcdf",
      focusedTextColor: "#d8dcdf",
      selectedBackgroundColor: "#171a1c",
      selectedTextColor: "#b7c58b",
      descriptionColor: "#7b838a",
      selectedDescriptionColor: "#8cb9bd",
      showDescription: true,
      showSelectionIndicator: true,
      wrapSelection: true,
    });
    this.panel.add(this.title);
    this.panel.add(this.search);
    this.panel.add(this.detail);
    this.panel.add(this.list);
    this.root.add(this.panel);
  }

  showList(title: string, options: SelectOption[], searchable: boolean, selected = 0): void {
    this.root.visible = true;
    this.title.content = title;
    this.search.visible = searchable;
    this.detail.visible = false;
    this.list.visible = true;
    this.list.options = options;
    this.list.setSelectedIndex(Math.min(selected, Math.max(options.length - 1, 0)));
    if (searchable) this.search.focus();
    else this.list.focus();
  }

  showDetail(title: string, content: string): void {
    this.root.visible = true;
    this.title.content = title;
    this.search.visible = false;
    this.list.visible = false;
    this.detail.visible = true;
    this.detail.content = content;
    this.root.focus();
  }

  hide(): void {
    this.root.visible = false;
    this.search.blur();
    this.list.blur();
  }
}
