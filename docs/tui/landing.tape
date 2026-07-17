Require cargo
Require python3

Set Shell "bash"
Set Width 1600
Set Height 900
Set FontSize 22
Set Padding 0
Set Margin 0
Set CursorBlink false
Set TypingSpeed 20ms
Set Theme { "name": "Eldritch", "black": "#171928", "red": "#f16c75", "green": "#37f499", "yellow": "#f1fc79", "blue": "#7081d0", "magenta": "#a48cf2", "cyan": "#04d1f9", "white": "#ebfafa", "brightBlack": "#292e42", "brightRed": "#f16c75", "brightGreen": "#37f499", "brightYellow": "#f7c67f", "brightBlue": "#abb4da", "brightMagenta": "#f265b5", "brightCyan": "#04d1f9", "brightWhite": "#ebfafa", "background": "#171928", "foreground": "#ebfafa", "selection": "#292e42", "cursor": "#f1fc79" }

Hide
Type "rm -rf /tmp/thndrs-showcase && mkdir -p /tmp/thndrs-showcase/.thndrs && cp tui/fixture/AGENTS.md /tmp/thndrs-showcase/AGENTS.md && cp tui/fixture/.thndrs/config.toml /tmp/thndrs-showcase/.thndrs/config.toml"
Enter
Sleep 200ms
Show
Type "unset NO_COLOR; USER=developer TERM=xterm-256color COLORTERM=truecolor CLICOLOR_FORCE=1 cargo run --quiet --manifest-path ../Cargo.toml -p thndrs -- --cwd /tmp/thndrs-showcase --no-mouse"
Enter
Wait+Screen /Ask for change/
Type "Add a health check to the API and cover it with a test."
Enter
Wait+Screen /Tests pass/
Sleep 300ms
Screenshot public/screenshot.png
Ctrl+C
Hide
Type "rm -rf /tmp/thndrs-showcase"
Enter
