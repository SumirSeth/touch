# Gesture app ↔ overlay protocol (v2)

`TrackpadGestures.exe` (C#, `trackpad/gestures/`) recognises finger gestures and **decides** what they do.
`TrackpadOverlay.exe` (native C++, `trackpad/overlay/`) draws finger-tracked animations and performs the
real shell action. They talk through one shared-memory block plus one wake event. Only the gesture app
writes; only the overlay reads.

## Objects

| Object | Name |
|---|---|
| Shared memory (page-file backed, 4096 bytes) | `Local\TrackpadGestures.Overlay` |
| Auto-reset event, signalled after every write | `Local\TrackpadGestures.OverlayWake` |
| Mutex held by the overlay for its whole lifetime (single instance + "am I running?" check) | `Local\TrackpadOverlay.Running` |

The gesture app launches `trackpad\overlay\bin\TrackpadOverlay.exe` at startup if it exists and the mutex
isn't held. It checks the mutex again at the start of each gesture to decide between overlay and fallback.

Either side may create them (`CreateFileMapping` / `CreateEvent` open the existing object if present).

## Layout (little-endian, offset in bytes)

| Offset | Type | Field | Meaning |
|---:|---|---|---|
| 0 | uint32 | `magic` | `0x32505754` ("TWP2"). Anything else: ignore the block (v1 was `0x31505754`). |
| 4 | uint32 | `seq` | Seqlock. Odd while the writer is updating, even when stable. |
| 8 | uint32 | `gesture` | 0 none, 1 Task View (swipe up), 2 show desktop (swipe down), 3 restore windows (swipe up while the desktop is shown by a previous gesture 2) |
| 12 | uint32 | `phase` | 0 idle, 1 tracking, 2 commit, 3 cancel, 4 armed |
| 16 | float32 | `progress` | Finger travel along the gesture direction ÷ 35 mm, measured from where the swipe engaged, so the first tracking frame is ≈ 0 (no jump). Can go below 0 or above 1. |
| 20 | float32 | `velocity` | Progress units per second, positive = towards committing. Smoothed over the last ~80 ms. |
| 24 | uint64 | `qpc` | `QueryPerformanceCounter` when written. |
| 32 | uint32 | `session` | Increments once per touch sequence (a new phase-4 or phase-1 sequence gets a new id). |

Reader: read `seq`; if odd, retry. Copy the fields, then re-read `seq`; if it changed, retry.

## Lifecycle

1. **Armed** (`phase=4`, `gesture=0`, new `session`): exactly three fingers just landed. No direction yet.
   The overlay should *silently* prepare (freeze the screen capture, enumerate windows, start captures,
   decode anything it needs) so it can show the first frame instantly when tracking begins. It must not
   show anything while armed. Many armed sessions end without a swipe (taps, 3-finger rests): `phase=0`
   or any different `session` means drop the preparation. Preparation must be cheap enough to do on every
   3-finger touch and must not cause visible artefacts (e.g. capture borders).
2. **Tracking** (`phase=1`, same `session` as the armed phase if there was one, `gesture` set,
   `progress` ≈ 0 at first): further frames about every 8 ms. If tracking arrives without a usable
   preparation, prepare as fast as possible.
3. **Commit** (`phase=2`): the app has decided the gesture commits. The overlay performs the action and
   finishes the animation from the given `progress`/`velocity` to 1. There is no decision left to the
   overlay.
4. **Cancel** (`phase=3`): released without committing, or abandoned. Animate back to 0 (using
   `velocity` for the settle), hide, perform no action.
5. The app writes `phase=0` after an armed session ends without a swipe. It does **not** overwrite a
   phase-2/3 terminal state with idle; the next session id replaces it.

### Commit rule (in the app)

`progress < 0.04` → cancel. Otherwise `velocity >= 0.8` → commit, `velocity <= -0.8` → cancel, otherwise
commit if `progress >= 0.4`. So moving back towards the start before lifting cancels, and a flick
forwards commits.

### Actions performed by the overlay on commit

| Gesture | Action | Preview |
|---|---|---|
| 1 Task View | Win+Tab | windows shrink into a grid over darkening wallpaper |
| 2 show desktop | Win+D | windows slide down off the screen |
| 3 restore windows | Win+D (the toggle brings the windows back) | the reverse of 2: windows rise from below into their places. Uses the window images cached from the last committed gesture 2; if none are cached, show nothing and just perform the action. |

The app handles these states itself without the overlay: swipe down while Task View is open (Escape on
commit) and swipe up while Task View is open (ignored).

Safety: if `phase` is 1 or 4 and `qpc` hasn't advanced for 1.5 s, treat it as cancelled.

## Fallback

If `TrackpadOverlay.exe` isn't running, the gesture app performs the discrete actions itself on commit
(Win+Tab, Win+D, Escape).
