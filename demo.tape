# m1nd demo tape — VHS (https://github.com/charmbracelet/vhs)
# Runtime: ~17 seconds
# Records terminal session showing m1nd semantic circuit simulator in action.

Output demo.gif

Set FontSize 14
Set Width 1200
Set Height 600
Set Theme "Catppuccin Mocha"
Set Padding 20
Set TypingSpeed 30ms
Set PlaybackSpeed 1

# -----------------------------------------------------------------
# 0. Title card — brief pause so GIF starts legible
# -----------------------------------------------------------------
Sleep 800ms

Type "# m1nd — semantic circuit simulator for AI agents"
Sleep 1200ms
Enter
Sleep 400ms

# -----------------------------------------------------------------
# 1. Health check — server alive
# -----------------------------------------------------------------
Type "./demo-run.sh health"
Sleep 400ms
Enter
Sleep 1200ms

# -----------------------------------------------------------------
# 2. Ingest codebase — the big number reveal
# -----------------------------------------------------------------
Type "./demo-run.sh ingest"
Sleep 400ms
Enter
Sleep 2200ms

# -----------------------------------------------------------------
# 3. Activate query — spreading activation ranked results
# -----------------------------------------------------------------
Type "./demo-run.sh activate"
Sleep 400ms
Enter
Sleep 1800ms

# -----------------------------------------------------------------
# 4. Learn — Hebbian feedback, edges strengthened
# -----------------------------------------------------------------
Type "./demo-run.sh learn"
Sleep 400ms
Enter
Sleep 1200ms

# -----------------------------------------------------------------
# 5. Hypothesize — structural claim verdict
# -----------------------------------------------------------------
Type "./demo-run.sh hypothesize"
Sleep 400ms
Enter
Sleep 2000ms

# Hold final frame
Sleep 1500ms
