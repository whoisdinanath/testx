# testx demo recording
# Usage: vhs demo.tape

Output demo.gif
Output demo.mp4

Set Shell "bash"
Set FontSize 15
Set Width 960
Set Height 580
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 35ms
Set PlaybackSpeed 1
Set CursorBlink false

# Setup: clean prompt, copy sandboxes to /tmp so no personal paths leak
# Remove flaky tests so demo output is always clean
Hide
Type `export PS1='❯ '`
Enter
Type "rm -rf /tmp/my-go-api /tmp/my-python-app /tmp/my-rust-lib"
Enter
Type "cp -r sandbox/go-project /tmp/my-go-api"
Enter
Type "cp -r sandbox/python-project /tmp/my-python-app"
Enter
Type "rm /tmp/my-python-app/test_flaky.py"
Enter
Type "cp -r sandbox/rust-project /tmp/my-rust-lib"
Enter
Type `sed -i '/test_flaky/,/^    }/d' /tmp/my-rust-lib/src/lib.rs`
Enter
Type "clear"
Enter
Show

Sleep 300ms

# ─── Scene 1: Version ───
Type "testx --version"
Enter
Sleep 1s

# ─── Scene 2: Supported languages ───
Type "testx list"
Sleep 300ms
Enter
Sleep 2.5s

# ─── Scene 3: Auto-detection ───
Type "# Auto-detects your test framework from project files"
Enter
Sleep 800ms

Type "testx detect -p /tmp/my-python-app"
Sleep 300ms
Enter
Sleep 2s

Type "testx detect -p /tmp/my-go-api"
Sleep 300ms
Enter
Sleep 2s

# ─── Scene 4: Run Go tests ───
Type "# Run tests with beautiful, unified output"
Enter
Sleep 800ms

Type "testx -p /tmp/my-go-api"
Sleep 300ms
Enter
Sleep 2.5s

# ─── Scene 5: Run Python tests ───
Type "testx -p /tmp/my-python-app"
Sleep 300ms
Enter
Sleep 3s

# ─── Scene 6: Run Rust tests ───
Type "testx -p /tmp/my-rust-lib"
Sleep 300ms
Enter
Sleep 3.5s

# ─── Scene 7: JSON output ───
Type "# Machine-readable output for CI pipelines"
Enter
Sleep 800ms

Type "testx -p /tmp/my-go-api -o json"
Sleep 300ms
Enter
Sleep 3s

# ─── Scene 8: Install ───
Type "# Install: cargo install testx-cli"
Enter
Sleep 2.5s
Sleep 2.5s
