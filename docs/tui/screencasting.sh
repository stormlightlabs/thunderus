# This is not a script but a reference
# list of commands for a screencasting workflow.

# Record
asciinema rec -c thndrs demo.cast

# Render
agg demo.cast demo.gif

# Screenshot at 12.5 seconds
ffmpeg -ss 12.5 -i demo.gif -frames:v 1 screenshot.png

# MP4
ffmpeg -i demo.gif \
    -c:v libx264 \
    -pix_fmt yuv420p \
    demo.mp4
