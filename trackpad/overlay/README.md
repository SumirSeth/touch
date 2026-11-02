# TrackpadOverlay

Native x64 C++ preview and shell-action helper. [PROTOCOL.md](PROTOCOL.md) is
the v2 contract; the C# app owns direction, state and commit/cancel decisions.
No downloaded packages, injection, private shell interfaces or registry writes.

## Build and run

```bat
bin\OverlayDriver.exe stop
build.cmd
start "" bin\TrackpadOverlay.exe
```

The script uses installed MSVC and Windows SDK 10.0.22000.0, building both
executables. Stop the helper before rebuilding. If a concurrent gesture app
relaunches it, stop that instance and temporarily rename the old executable
inside `bin/`. A named mutex prevents duplicate instances.

## Lifecycle

- **Armed (4):** silently freeze the primary monitor, enumerate visible,
  non-minimized applications, latch geometry and prepare a hidden zero frame.
  DWM-cloaked windows, including other virtual desktops, are excluded. No
  synchronous Explorer membership or wallpaper queries run on finger landing.
  WGC starts incrementally, one window per armed update, checking protocol input
  between windows. Screen crops are immediately usable as fallback.
- **Tracking (1):** reuse the same session's preparation and present the supplied
  progress. There is no extra forced zero frame before the requested position.
  If preparation was missed, acquire once.
- **Commit (2):** send Win+Tab or Win+D immediately, regardless of progress or
  velocity; finish the preview toward 1 concurrently. No overlay commit rule.
- **Cancel (3):** settle to 0 and hide, without shell input. The 90–220 ms settle
  uses remaining distance and supplied velocity.
- Idle/new session discard preparation. Stale armed/tracking QPC cancels after
  1.5 seconds. A timed-out session cannot reopen with the same ID. Display/DPI
  changes and fatal rendering errors hide and exit.

Borderless access is requested once at startup using
`GraphicsCaptureAccess::RequestAccessAsync(Borderless)`. Until allowed, use only
screen crops. Every WGC session sets `IsBorderRequired(false)` before starting;
failed/denied access must never add a yellow capture border on a three-finger
tap. Failed captures retain their last image or crop. Armed polling does not
present frames.

Rendering uses D3D11, Direct2D, DirectComposition, a two-buffer swapchain and
maximum frame latency one. The hidden no-activate, click-through HWND uses
`WDA_EXCLUDEFROMCAPTURE`. Idle has no captures or polling timer, except for a
pending Task View animation-setting restoration deadline. Wallpaper is decoded
at initialization and cached; restart after wallpaper changes.

Task View uses aspect-preserving, equal-height packed rows fitted to native
thumbnail measurements on this machine. It approximates native layout, omitting
minimized windows, labels, shadows and the virtual-desktop strip.

Show desktop slides windows below the monitor. Commit caches immutable window
images, original geometry and the frozen screen before minimization; no further
capture frames overwrite them during hand-off. Gesture 3 reverses this animation
over a fresh desktop background. Cancelling restore preserves both the desktop
and cache. With no cached windows it draws nothing and still sends Win+D on
commit. Preview/cancel never moves any real application window.

## Hand-off

Win+D hides on the original windows' visibility/minimization state, with a
300 ms escape bound for unusual windows. There is no mandatory settle, hold or
fade after observing native visibility.

Task View draws above ordinary topmost windows. Its foreground host is class
`XamlExplorerHostIslandWindow`, title `Task View`, but foreground precedes first
paint. A small independent GDI screen probe checks when its wallpaper replaces
the original corner, excluding our overlay. This is a heuristic: a desktop
corner can remain unchanged. A nominal 450 ms escape bound replaces the old 1.2-second
fallback; no readiness dwell or extra fade is added.

Native screenshot probes on this machine showed intermediate zoom geometry at
about 400 ms with client-area animation enabled, but a direct transition to the
finished grid with it disabled. Around Win+Tab the helper therefore temporarily
sets `SPI_SETCLIENTAREAANIMATION` false with `fWinIni=0`. It restores the prior
value after 450 ms, including while hidden. Replacement, SendInput failure,
normal exit, rendering exceptions and display-change cleanup restore it too.
No preference is persisted or broadcast. Forced process termination/OS crash
cannot execute in-process cleanup.

## Driver and evidence

Use one protocol writer at a time; avoid real touchpad input during driver runs.
The driver preserves/increments the shared session, arms for 220 ms before
tracking, refreshes QPC every 8 ms and leaves terminals published. Timer
resolution is requested only while the driver runs.

```bat
bin\OverlayDriver arm 400
bin\OverlayDriver up scrub
bin\OverlayDriver up cancel
bin\OverlayDriver up commit
bin\OverlayDriver down commit
bin\OverlayDriver back cancel
bin\OverlayDriver back commit
bin\OverlayDriver up 0.5 hold 1000
bin\OverlayDriver status
```

`back` is gesture 3. `restore` sends a raw Win+D toggle: use only when desktop
was actually shown. `escape` closes Task View. `low-commit` commits at 0.01 with
negative velocity, checking that the producer's decision is authoritative.

`verify` covers cancel/commit, show desktop, cancelled restore and committed
restore, closing the Task View it opens. It assumes Task View/show desktop are
initially closed and no concurrent input. `safety` checks armed/tracking
watchdogs, replacement during cancellation and out-of-range progress.
`controls` creates two temporary responsive windows, records preview shots and
destroys them. `up shots`/`down shots` use existing windows. `armed-evidence`
records hidden armed status and an independent borderless screenshot.

`native-probe` opens Task View twice, with/without client animation, takes five
timed raw frames (encoding PNGs after closing Task View), and reports native UI
Automation thumbnail rectangles. It restores the animation setting, closes Task
View with Escape, and never selects or switches a virtual desktop. It skips if
Task View is already open.

`TrackpadOverlay.exe --measure` logs write-to-first-frame and tracking timing
through `DwmFlush`, acquisition stages, commit-to-SendInput and action-to-hide.
These are software/compositor measurements, not physical input-to-photon.
Normal rendering does not call `DwmFlush`. Synchronous diagnostic PNG encoding
must be excluded from warm tracking statistics. Logs/screenshots are gitignored
in `bin/` and `test-shots/` and may contain private screen content. The overlay
log is overwritten at launch.

Verification on this machine (2026-10-30, 1920x1200): the final full driver run
recorded 10 prepared starts at 8.14–22.36 ms through first-frame `DwmFlush`;
arming acquisition was 33.55–61.41 ms, with WGC startup logged separately.
Commit-to-SendInput was 18.62 ms for Task View, 15.84 ms for show desktop,
and 11.49 ms for restore. Action-to-hide was respectively 354.31, 21.98 and
35.69 ms, with readiness observed in each case. Native calls/scheduling can
overrun nominal deadlines; an earlier Task View run took 536 ms to hide.
These are short scripted measurements, not physical finger-to-photon results.

`test-shots/measured-v2.log`, `verification-v2.txt`, `safety-v2.txt`,
`armed-v2.txt`, `native-probe-v2.txt` and their PNGs retain the local evidence.
Armed status reported a hidden HWND; its screenshot had no capture border.
Inspected gesture-3 PNGs show windows rising at 0.3, 0.6 and 1.0. Cancellation
kept original windows minimized, commit restored them, and all safety cases
ended hidden with no shell action. The animation flag read back enabled after
the run. Real-finger behavior with the concurrent C# producer still needs the
owner's confirmation; no virtual desktops were switched by these tests.

The final build also accepted `low-commit` at progress 0.01/velocity -4,
restored client animations on WM_CLOSE during the hand-off, and ignored the
retained terminal after restart (`commit-authority-exit-v2.log`). The normal
build was left running, hidden and idle, after a final cancellation check.

## Protocol and limits

The v2 ABI is unchanged. There is no consumption acknowledgement: a terminal
present at process startup may already have executed. The helper ignores that
initial terminal to avoid replaying Win+D/Win+Tab. A new session's commit while
running still executes if earlier tracking was missed. Launching only after a
terminal was published cannot guarantee execution without acknowledgement.

Only the primary monitor is supported. Crop fallbacks can include occluding
windows. Wallpaper reconstruction omits some icons, taskbar, translucent
backdrops and shadows until native content takes over. Task View readiness is
not a shell presentation fence. Protected content, elevated apps, device loss,
lock/UAC, Explorer restarts, other monitors and DPI modes are not broadly tested.
Physical touchpad/native gesture suppression belongs to the C# producer.
