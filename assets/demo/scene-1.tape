# Scene 1, captioned by record.sh: search every folder, press Enter, the shell is there.
Output "/tmp/dm-rec/scene-1/"
Source "assets/demo/setup.tape"
# Off camera: one run of ff so the folder index is cached for every scene
Type "cd ~/Projects && ff" Enter Sleep 8s
Type "q" Sleep 500ms
Type "clear" Enter Sleep 500ms
Show

Sleep 400ms
Type "ff" Sleep 150ms Enter Sleep 300ms
Type "p" Sleep 50ms
Space Sleep 100ms Type "/" Sleep 150ms
Type@110ms "comp" Sleep 1s
Type@110ms " geoid" Sleep 1.2s
Enter Sleep 500ms
Type "ls" Enter Sleep 1.4s
