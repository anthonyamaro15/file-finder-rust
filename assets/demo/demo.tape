# The README GIF. Run assets/demo/record.sh rather than this tape: it builds ff,
# makes the home folder below and turns these frames into assets/demo.gif.
Output "/tmp/ff-demo/frames/"

Set Shell "bash"
Set FontFamily "JetBrainsMono Nerd Font Mono"
Set FontSize 16
Set Width 1320
Set Height 620
Set Padding 16
Set Framerate 25
Set TypingSpeed 70ms
Set Theme { "name": "ff-onedark", "black": "#181b20", "red": "#e55561", "green": "#8ebd6b", "yellow": "#e2b86b", "blue": "#4fa6ed", "magenta": "#bf68d9", "cyan": "#48b0bd", "white": "#a0a8b7", "brightBlack": "#535965", "brightRed": "#e55561", "brightGreen": "#8ebd6b", "brightYellow": "#e2b86b", "brightBlue": "#4fa6ed", "brightMagenta": "#bf68d9", "brightCyan": "#48b0bd", "brightWhite": "#dfe3ea", "background": "#1f2329", "foreground": "#a0a8b7", "selection": "#323641", "cursor": "#a0a8b7" }

# Off camera: open ff in a project inside the made-up home folder
Hide
Type "export HOME=$(cd /tmp && pwd -P)/ff-demo/home && cd ~/Projects/weather-app && clear" Enter
Sleep 500ms
Type "ff" Enter
Sleep 3s
Show

# 1. Browse: the preview follows the highlight
Sleep 1s
Type "j" Sleep 350ms
Type "j" Sleep 400ms
Type "j" Sleep 800ms
Type "j" Sleep 250ms
Type "j" Sleep 450ms
Type "l" Sleep 800ms
Type "j" Sleep 650ms
Type "j" Sleep 1s

# 2. Global search finds a folder nine levels down
Space Sleep 100ms Type "/" Sleep 300ms
Type@85ms "signed" Sleep 1.5s
Escape Sleep 250ms
Type "l" Sleep 1.8s

# 3. Move the highlighted file to another folder, picked by name
Type "M" Sleep 300ms
Type@70ms "north/arch" Sleep 1s
Enter Sleep 1.4s

# ...and jump there to see it arrived
Space Sleep 100ms Type "/" Sleep 200ms
Type@70ms "north/arch" Sleep 700ms
Escape Sleep 200ms
Type "l" Sleep 2.5s
