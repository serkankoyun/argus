# Records the README demo and per-view screenshots (English).
Output docs/media/demo-en.gif

Set Shell "bash"
Set FontSize 14
Set Width 1600
Set Height 900
Set Padding 12
Set Theme "GitHub Dark"
Set TypingSpeed 80ms

Hide
Type "./target/release/argus --demo --lang en"
Enter
Sleep 4s
Show

Sleep 3s
Screenshot docs/media/overview-en.png
Type "5"
Sleep 5s
Screenshot docs/media/graph-en.png
Type "6"
Sleep 5s
Screenshot docs/media/room-en.png
Type "7"
Sleep 5s
Screenshot docs/media/lanes-en.png
Type "1"
Sleep 1s
Down
Type "3"
Sleep 3s
Screenshot docs/media/agent-en.png
Type "1"
Sleep 2s
Hide
Type "q"
