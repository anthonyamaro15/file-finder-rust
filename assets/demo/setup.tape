# Shared by the three scene tapes (scene-*.tape): window, font and colors, then, off
# camera, the made-up home folder, the shell function and a prompt that shows the
# folder. Run assets/demo/record.sh rather than any tape directly.
Set Shell "bash"
Set FontFamily "JetBrainsMono Nerd Font Mono"
Set FontSize 26
Set Width 1100
Set Height 704
Set Padding 12
Set Framerate 25
Set TypingSpeed 90ms
Set CursorBlink false
Set Theme { "name": "ff-onedark", "black": "#181b20", "red": "#e55561", "green": "#8ebd6b", "yellow": "#e2b86b", "blue": "#4fa6ed", "magenta": "#bf68d9", "cyan": "#48b0bd", "white": "#a0a8b7", "brightBlack": "#535965", "brightRed": "#e55561", "brightGreen": "#8ebd6b", "brightYellow": "#e2b86b", "brightBlue": "#4fa6ed", "brightMagenta": "#bf68d9", "brightCyan": "#48b0bd", "brightWhite": "#dfe3ea", "background": "#1f2329", "foreground": "#a0a8b7", "selection": "#323641", "cursor": "#a0a8b7" }

Hide
Type `export HOME=$(cd /tmp && pwd -P)/dm-rec/home CLICOLOR=1 PS1='\[\e[1;34m\]\w\[\e[0m\] ❯ ' && eval "$(ff init bash)"` Enter
