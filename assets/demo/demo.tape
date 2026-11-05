# The README GIF. Run assets/demo/record.sh rather than this tape: it builds ff,
# makes the home folder below and turns these frames into assets/demo.gif.
Output "/tmp/ff-demo/frames/"

Set Shell "bash"
Set FontFamily "JetBrainsMono Nerd Font Mono"
Set FontSize 20
Set Width 1224
Set Height 540
Set Padding 16
Set Framerate 25
Set TypingSpeed 70ms
Set Theme { "name": "ff-onedark", "black": "#181b20", "red": "#e55561", "green": "#8ebd6b", "yellow": "#e2b86b", "blue": "#4fa6ed", "magenta": "#bf68d9", "cyan": "#48b0bd", "white": "#a0a8b7", "brightBlack": "#535965", "brightRed": "#e55561", "brightGreen": "#8ebd6b", "brightYellow": "#e2b86b", "brightBlue": "#4fa6ed", "brightMagenta": "#bf68d9", "brightCyan": "#48b0bd", "brightWhite": "#dfe3ea", "background": "#1f2329", "foreground": "#a0a8b7", "selection": "#323641", "cursor": "#a0a8b7" }

# Off camera: open ff in a project inside the made-up home folder, then open src/
Hide
Type "export HOME=$(cd /tmp && pwd -P)/ff-demo/home && cd ~/Projects/weather-app && clear" Enter
Sleep 500ms
Type "ff" Enter
Sleep 3s
# Step into src/ (the last item) so the first frame is highlighted code
Type "G" Sleep 300ms
Type "l" Sleep 800ms
Show

# 1. The preview shows the highlighted file, with syntax highlighting
Sleep 1.8s

# 2. Root search looks through every folder under home, from anywhere...
Space Sleep 100ms Type "/" Sleep 200ms
Type@60ms "contracts" Sleep 1.3s

# ...and narrows to a folder nine levels down
Type@70ms "/signed" Sleep 1.1s
Escape Sleep 150ms
Type "l" Sleep 2.2s

# 3. Move the file into another folder, picked by name the same way
Type "M" Sleep 300ms
Type@70ms "north/arch" Sleep 1.2s
Enter Sleep 2.2s
