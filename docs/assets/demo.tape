# demo.tape — regenerable source for docs/assets/demo.gif
# Render:  vhs docs/assets/demo.tape   (charmbracelet/vhs)
#
# What it records: a REAL m1nd agent session (north → seek → memorize) captured from a
# live owner (m1nd-mcp 1.3.0, 6,453-node graph over this repo, full_trust). The outputs
# in docs/assets/demo-replay.sh are verbatim-captured JSON, trimmed to width — no faked
# capabilities. Soft-proof aesthetic: warm porcelain ground, no neon.

Output docs/assets/demo.gif

Require bash

# — soft-proof look: warm porcelain background, calm ink, generous type —
Set Theme "rose-pine-dawn"
Set FontSize 18
Set Width 1120
Set Height 720
Set Padding 28
Set Margin 14
Set MarginFill "#f2e9e1"
Set BorderRadius 10
Set WindowBar Colorful
Set WindowBarSize 32
Set PlaybackSpeed 1.0
Set TypingSpeed 40ms

Hide
Type "bash docs/assets/demo-replay.sh"
Enter
Show

# the replay script self-paces with sleeps; give it room to breathe, then hold the final frame
Sleep 14s
