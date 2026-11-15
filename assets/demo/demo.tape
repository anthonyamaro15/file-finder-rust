# The README GIF. Run assets/demo/record.sh rather than this tape: it builds ff,
# makes the home folder below and turns these frames into assets/demo.gif.
Output "/tmp/dm-rec/frames/"

Set Shell "bash"
Set FontFamily "JetBrainsMono Nerd Font Mono"
Set FontSize 28
Set Width 1280
Set Height 720
Set Padding 12
Set Framerate 25
Set TypingSpeed 90ms
Set CursorBlink false
Set Theme { "name": "ff-onedark", "black": "#181b20", "red": "#e55561", "green": "#8ebd6b", "yellow": "#e2b86b", "blue": "#4fa6ed", "magenta": "#bf68d9", "cyan": "#48b0bd", "white": "#a0a8b7", "brightBlack": "#535965", "brightRed": "#e55561", "brightGreen": "#8ebd6b", "brightYellow": "#e2b86b", "brightBlue": "#4fa6ed", "brightMagenta": "#bf68d9", "brightCyan": "#48b0bd", "brightWhite": "#dfe3ea", "background": "#1f2329", "foreground": "#a0a8b7", "selection": "#323641", "cursor": "#a0a8b7" }

# Off camera: the shell function, a prompt that shows the folder, and one run of ff
# so the folder index is already cached
Hide
Type `export HOME=$(cd /tmp && pwd -P)/dm-rec/home CLICOLOR=1 PS1='\[\e[1;34m\]\w\[\e[0m\] ❯ ' && cd ~/Projects/weather-app && eval "$(ff init bash)"` Enter
Type "ff" Enter Sleep 8s
Type "q" Sleep 500ms
Type "clear" Enter Sleep 500ms
Show

# 1. Search every folder under home: one word finds dozens of components folders,
#    a second word, in any order, leaves one
Sleep 300ms
Type "ff" Sleep 200ms Enter Sleep 400ms
Type "p" Sleep 50ms
Space Sleep 100ms Type "/" Sleep 150ms
Type@110ms "comp" Sleep 1s
Type@110ms " geoid" Sleep 1.2s

# 2. Enter: ff quits and the shell is in that folder
Enter Sleep 600ms
Type "ls" Enter Sleep 1.2s

# 3. Initials work too
Type "ff" Sleep 200ms Enter Sleep 400ms
Type "p" Sleep 50ms
Space Sleep 100ms Type "/" Sleep 150ms
Type@140ms "ffr" Sleep 1s
Enter Sleep 600ms
Type "ls" Enter Sleep 1.2s
