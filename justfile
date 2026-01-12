format:
    cargo fmt

alias fmt := format

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

alias cmt := find-comments

test:
    cargo test --quiet
