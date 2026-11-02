# Why replaying real 3-finger swipes as a synthetic 4-finger swipe mostly fails (2026-10-30)

Goal: 3-finger left/right on the real touchpad drives Windows' native, finger-tracked desktop switch by replaying
the motion on a gesture-only synthetic touchpad (`gestures/VirtualTouchpad.cs`, see 04-synthetic-touchpad-test.md).
Windows' own 3-finger swipes were set to Nothing.

## Findings, in the order they were found

1. **Repeated timestamps are rejected.** `InjectSyntheticPointerInput` fails with error 87 when `dwTime` equals the
   previous frame's. `Environment.TickCount` only advances every ~16 ms and frames arrive every ~8 ms, so every
   injection failed. Fixed: strictly increasing timestamps.
2. **Synthetic touchpads produce raw input.** Our raw-input sink (usage page 0x0D, usage 0x05) received the virtual
   touchpad's own contacts and read them as the real hand changing finger count, which ended the replay a few ms
   after it started. Synthetic devices are named `\\?\Microsoft HID RID\000D_0005\NN` (real ones `\\?\HID#...`);
   they're now ignored.
3. **Resting real fingers don't block it.** Guided trials (`scratch-04/FingerTrials.cs`): with 0, 1, 2 or 3 real
   fingers resting still, a synthetic swipe switched desktops every time.
4. **Two synthetic touchpads:** a cursor-type synthetic pad with 3 moving fingers blocked a second pad's 4-finger
   swipe unless the second pad started moving within ~1.5 mm of the first (`scratch-04/TwoPads.cs`). Starting the
   replay when the fingers land, mirroring from the first frame, didn't fix the real touchpad though.
5. **The deciding factor is how the real fingers land** (tested by the user):
   - three fingers landing together, then swiping: **fails**, every time
   - one finger then two, or two then one, then swiping: **works** (native tracked animation, partial + cancel)
   - setting 3-finger taps to Nothing as well: still fails
   - turning the real touchpad off for Windows mid-touch (`SPI_SETTOUCHPADPARAMETERS`, touchpadEnabled = 0;
     raw input keeps arriving): still fails

## Conclusion

Windows' touchpad gesture engine classifies a touch when the fingers land, by how many land together. A touch that
starts as three simultaneous fingers is held as a 3-finger gesture until every finger lifts, even when every 3-finger
action is Nothing, and even if the touchpad is disabled mid-touch. While it holds that touch, it ignores gestures from
other touchpads. A touch that starts with one or two fingers begins as pointer/pan and is not reclassified when more
fingers join, so the synthetic touchpad's 4-finger swipe gets through.

The only user-mode way around it would be disabling Windows' touchpad handling permanently and reimplementing pointer,
scrolling and clicks from raw input, which isn't worth it. Native 4-finger swipes already give the tracked desktop
switch. The 3-finger replay remains in the app (it works for staggered landings) and is harmless otherwise.

Not tested: whether a filter driver could rewrite the reported contact count (out of scope: no drivers, no
third-party code).
