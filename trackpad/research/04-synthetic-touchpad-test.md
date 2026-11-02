# Synthetic touchpad test (2026-10-29)

Probe: `scratch-04/InjectProbe.cs`. Creates a virtual Precision Touchpad with `CreateSyntheticPointerDevice2`
(PT_TOUCHPAD, 5 contacts, 12690×7930 himetric, `SDCO_PHYSICAL_SIZE`) and injects 4-finger frames with
`InjectSyntheticPointerInput` every 8 ms, following Microsoft's Precision Touchpad programming guide
(flags INRANGE|INCONTACT|CONFIDENCE while touching, CONFIDENCE only to lift; positions in `ptHimetricLocation`).
Screenshots were captured mid-gesture while the virtual fingers were held still.

Windows settings at the time: four-finger swipes = "switch desktops and show desktop" (default).

| Gesture | Mid-gesture (fingers held at ~40%) | Drag back + lift | Full swipe |
|---|---|---|---|
| 4 fingers left/right | **Tracked:** current desktop slid ~40% left, next desktop's wallpaper visible on the right | Cancelled, stayed on original desktop | Switched desktop; reverse swipe switched back |
| 4 fingers up (Task View) | **Not tracked:** Task View already fully open | Task View closed, screen identical to start | (not run) |
| 4 fingers down (show desktop) | **Not tracked:** all windows already hidden | Windows restored | (not run) |

## Conclusions

- A normal user-mode process can drive Windows' native, finger-tracked desktop-switch animation with no
  injection into Explorer and no third-party code. Partial progress, hold and cancel all work.
- Task View and show desktop are threshold-triggered (reversible during the gesture, but not scrubbed).
  Tracked versions of those need our own overlay (see 02-overlay-compositor.md).
- Remapping is possible across finger counts: e.g. set Windows' 3-finger swipes to Nothing, recognise real
  3-finger swipes in our app, and inject a synthetic 4-finger swipe so Windows' native tracked animation runs.
  (Disabling the same finger count natively would likely disable the synthetic one too; untested.)
- Not yet tested: the `SDCO_TOUCHPAD_GESTURE_ONLY` device (avoids accidental cursor movement), and latency
  when relaying live finger motion rather than scripted motion.
