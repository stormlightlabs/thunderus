# Thunderus - Flask Web Interface

Terminal AI Assistant web interface built with Flask and Jinja2 templates.

## Quick Start

```bash
# Install dependencies with uv
uv sync

# Run the development server
uv run python app.py

# Or use the script entry point
uv run thunderus
```

Then open <http://127.0.0.1:5000> in your browser.

## Template Inheritance

All pages extend `base.html` which provides:

- Common HTML structure (head, body)
- Navigation bar with active state
- Terminal window chrome
- CSS variable theme system

### Blocks Available

- `title` - Page title (suffixes with " - Terminal AI Assistant")
- `extra_css` - Additional page-specific styles
- `content_style` - Content div attributes (e.g., for padding)
- `content` - Main page content
- `scripts` - Page-specific JavaScript

### Example Page

```html
{% extends "base.html" %} {% block title %}My Page{% endblock %} {% block
extra_css %}
<style>
  .my-component {
    color: var(--accent-cyan);
  }
</style>
{% endblock %} {% block content %}
<div class="my-component">Hello World</div>
{% endblock %}
```

## Design System

### CSS Variables (Oxocarbon Dark)

| Variable           | Hex     | Usage                    |
| ------------------ | ------- | ------------------------ |
| `--accent-cyan`    | #33b1ff | Primary actions, prompts |
| `--accent-pink`    | #ff7eb6 | Secondary accents, code  |
| `--accent-purple`  | #be95ff | User indicators          |
| `--accent-green`   | #42be65 | Success states           |
| `--accent-yellow`  | #f1c21b | Warnings, folders        |
| `--accent-red`     | #fa4d56 | Errors                   |
| `--bg-primary`     | #161616 | Main background          |
| `--bg-secondary`   | #1c1c1c | Panels, headers          |
| `--bg-tertiary`    | #262626 | Cards, inputs            |
| `--bg-terminal`    | #0c0c0c | Terminal content         |
| `--text-primary`   | #f4f4f4 | Primary text             |
| `--text-secondary` | #c6c6c6 | Secondary text           |
| `--text-muted`     | #8d8d8d | Timestamps, hints        |
| `--border-color`   | #393939 | Borders, dividers        |
