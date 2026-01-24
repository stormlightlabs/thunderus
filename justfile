format:
    cargo fmt


# Lint AND fix
lint:
    cargo clippy --fix --allow-dirty

compile:
    cargo check

# Overall code quality check
check: format lint compile test

# Finds comments
find-comments:
    rg -n --pcre2 '^\s*//(?![!/])' -g '*.rs'


test:
    cargo test --quiet

# Run doc site dev server
docs:
    pnpm -C docs docs:dev

# Install doc site dependencies
docs-install:
    pnpm -C docs install

# Build doc site
docs-build:
    pnpm -C docs docs:build

# Preview doc site production build
docs-preview:
    pnpm -C docs docs:preview

alias docs-i := docs-install
alias docs-b := docs-build
alias docs-p := docs-preview
alias fmt := format
alias cmt := find-comments
