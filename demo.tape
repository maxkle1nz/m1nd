# m1nd demo tape — VHS (https://github.com/charmbracelet/vhs)
# Runtime: ~40 seconds
# Records terminal session showing m1nd semantic circuit simulator in action.

Output demo.gif

Set FontSize 14
Set Width 1200
Set Height 600
Set Theme "Catppuccin Mocha"
Set Padding 20
Set TypingSpeed 40ms
Set PlaybackSpeed 1

# -----------------------------------------------------------------
# 0. Title card — let the viewer read it
# -----------------------------------------------------------------
Sleep 1500ms

Type "# m1nd — semantic circuit simulator for AI agents"
Sleep 2000ms
Enter
Sleep 800ms

# -----------------------------------------------------------------
# 1. Health check — server alive (quick, just shows it's running)
# -----------------------------------------------------------------
Type "./demo-run.sh health"
Sleep 500ms
Enter
Sleep 2500ms

# -----------------------------------------------------------------
# 2. Ingest codebase — the big number reveal (KEY moment)
# -----------------------------------------------------------------
Type "./demo-run.sh ingest"
Sleep 500ms
Enter
Sleep 4500ms

# -----------------------------------------------------------------
# 3. Activate query — spreading activation ranked results (MAIN event)
# -----------------------------------------------------------------
Type "./demo-run.sh activate"
Sleep 500ms
Enter
Sleep 5000ms

# -----------------------------------------------------------------
# 4. Learn — Hebbian feedback, edges strengthened
# -----------------------------------------------------------------
Type "./demo-run.sh learn"
Sleep 500ms
Enter
Sleep 3000ms

# -----------------------------------------------------------------
# 5. Hypothesize — structural claim verdict (CLIMAX)
# -----------------------------------------------------------------
Type "./demo-run.sh hypothesize"
Sleep 500ms
Enter
Sleep 5000ms

# Hold final frame — let the verdict sink in
Sleep 3000ms
