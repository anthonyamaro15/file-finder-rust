# The index manager, appended to demo.tape's recording in target/demo.mp4 (for
# Reddit and other sites that take video). record.sh runs it after demo.tape.
Output "/tmp/dm-rec/frames-index/"

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

# Off camera: the shell function and the prompt; demo.tape already cached the index
Hide
Type `export HOME=$(cd /tmp && pwd -P)/dm-rec/home CLICOLOR=1 PS1='\[\e[1;34m\]\w\[\e[0m\] ❯ ' && cd ~/Projects/file-finder-rust && eval "$(ff init bash)"` Enter
Type "clear" Enter Sleep 500ms
Show

Sleep 300ms
# 4. The index manager: the Go module cache fills most of the index; switch it off
Type "ff" Sleep 200ms Enter Sleep 400ms
Space Sleep 100ms Type "i" Sleep 2s
Space Sleep 1.8s
Enter Sleep 2s
