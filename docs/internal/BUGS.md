# Bugs

Codex Usage Quota can be parsed through these headers:

```text
x-codex-primary-used-percent
x-codex-primary-window-minutes
x-codex-primary-reset-at
x-codex-secondary-used-percent
x-codex-secondary-window-minutes
x-codex-secondary-reset-at
x-codex-credits-has-credits
x-codex-credits-unlimited
x-codex-credits-balance
```

Note: not formally documented

---

- We should render the reasoning level
  - It should be toggleable/cycleable
- Entering `/model` and selecting a new model keeps `/model` in the prompt
