# gestures_engine

Spec deviations: none in the Engine API or config JSON. `Action::None` is allowed
on either trigger kind as an explicit unbinding. Invalid configurations (including
semantic validation failures) are backed up and replaced with defaults just like
malformed JSON. `AdjustTarget` is serialized within `Action` using the flat shape
specified by the contract. The recognizers share a pure arbitration state machine
rather than exposing one public type per prototype recognizer.

`gestures::Recognizer` accepts complete `Frame`s with monotonic millisecond times
and returns `GestureEvent`s. It never calls Windows. Coordinates and edge steps
are millimetres; knob steps are degrees. Bindings are captured for each touch.
A claimed touch stays claimed until every finger lifts. Three-finger drag disables
three-finger taps and swipes. Pause, disabled config, device changes, window exit,
and engine destruction release cursor clipping and the injected drag button.

Raw input is received on an invisible top-level window using RIDEV_INPUTSINK.
HID descriptor physical dimensions and units scale coordinates, and each device
has independent hybrid-report assembly. Synthetic Microsoft HID RID devices are
ignored. Raw input never suppresses Windows gestures; configure Windows' matching
three/four finger gestures to Nothing. Registry conflict detection only reads.
Actions run on a worker; queued brightness deltas are coalesced and WMI brightness
is read again before writing to respect external changes. WMI is for built-in
panels supporting WmiMonitorBrightness; failures are EngineEvent::Error.

Run `cargo run -p gestures_engine --example run` for manual hardware testing.
This uses saved bindings and **can inject keys**. Do not run while working.
The automated tests use only synthetic frames and never inject input.

Hardware verification still required: the ASUS hybrid descriptor/report stream,
physical-unit scaling, lift reports, real button usages, ClipCursor interaction,
SendInput behavior in target applications, brightness and registry conflicts.
Windows owns normal pointer movement and scrolling throughout.
