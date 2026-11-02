using System;
using System.Runtime.InteropServices;

// A synthetic Precision Touchpad that replays a 4-finger swipe, so Windows runs its own native, finger-tracked
// gesture (e.g. the virtual-desktop switch). Gesture-only: Windows never turns its input into cursor movement or clicks.
// See trackpad/research/04-synthetic-touchpad-test.md.
class VirtualTouchpad : IDisposable {
    const uint PT_TOUCHPAD = 5, POINTER_FEEDBACK_NONE = 3, SDCO_PHYSICAL_SIZE = 1, SDCO_TOUCHPAD_GESTURE_ONLY = 2;
    const uint INRANGE = 0x2, INCONTACT = 0x4, CONFIDENCE = 0x4000;
    const int InfoSize = 152; // sizeof(POINTER_TYPE_INFO) on x64: type + padding + POINTER_TOUCH_INFO
    const int Fingers = 4;
    const int MinTouchMs = 350;
    static readonly int[] FingerX = { -2250, -750, 750, 2250 }; // himetric (0.01 mm) offsets from the centroid
    static readonly int[] FingerY = { 400, 0, 0, 400 };

    readonly int width, height; // himetric
    IntPtr device, buffer;
    bool touching, failed;
    int startX, startY, x, y;
    uint time;
    int began; // TickCount at Begin
    System.Windows.Forms.Timer liftLater;

    public VirtualTouchpad(double widthMm, double heightMm) {
        width = (int)(widthMm * 100); height = (int)(heightMm * 100);
    }

    public bool Touching { get { return touching; } }

    // Puts 4 fingers down. dirX/dirY (-1, 0, +1) is the expected swipe direction, so the hand starts on the opposite
    // side of the pad and has room to travel the whole way.
    public void Begin(int dirX, int dirY) {
        if (device == IntPtr.Zero) Create();
        Lift();
        int marginX = 2450, marginY = 1000;
        startX = dirX < 0 ? width - marginX : dirX > 0 ? marginX : width / 2;
        startY = dirY < 0 ? height - marginY : dirY > 0 ? marginY : height / 2;
        x = startX; y = startY; failed = false;
        touching = true; began = Environment.TickCount;
        Inject(true);
    }

    // Moves the hand to the start position plus the given offset in mm (clamped to the pad).
    public void MoveTo(double dxMm, double dyMm) {
        if (!touching) return;
        x = Clamp(startX + (int)(dxMm * 100), 2450, width - 2450);
        y = Clamp(startY + (int)(dyMm * 100), 1000, height - 1000);
        Inject(true);
    }

    // Lifts the fingers where they are; Windows decides whether the gesture commits.
    // A brief touch that hardly moved would look like a 4-finger tap, so that is held until MinTouchMs first.
    public void End() {
        if (!touching) return;
        int wait = MinTouchMs - (Environment.TickCount - began);
        if (wait <= 0 || Math.Abs(x - startX) + Math.Abs(y - startY) >= 300) { Lift(); return; }
        if (liftLater == null) {
            liftLater = new System.Windows.Forms.Timer();
            liftLater.Tick += (s, e) => Lift();
        }
        liftLater.Interval = wait;
        liftLater.Start();
    }

    void Lift() {
        if (liftLater != null) liftLater.Stop();
        if (!touching) return;
        Inject(false);
        touching = false;
    }

    // Slides back to the start before lifting, so Windows cancels the gesture.
    public void Cancel() {
        if (!touching) return;
        int fromX = x, fromY = y;
        for (int i = 1; i <= 6; i++) {
            x = fromX + (startX - fromX) * i / 6; y = fromY + (startY - fromY) * i / 6;
            Inject(true);
        }
        Lift();
    }

    void Create() {
        var p = new Native.SYNTHETIC_DEVICE_CREATION_PARAMS {
            PointerType = PT_TOUCHPAD, MaxCount = 5, FeedbackMode = POINTER_FEEDBACK_NONE, Monitor = IntPtr.Zero,
            Width = (uint)width, Height = (uint)height, Options = SDCO_PHYSICAL_SIZE | SDCO_TOUCHPAD_GESTURE_ONLY,
        };
        device = Native.CreateSyntheticPointerDevice2(ref p);
        if (device == IntPtr.Zero) throw new InvalidOperationException("CreateSyntheticPointerDevice2 failed: " + Marshal.GetLastWin32Error());
        Program.Log("virtual touchpad created");
        buffer = Marshal.AllocHGlobal(InfoSize * Fingers);
    }

    void Inject(bool contact) {
        uint flags = contact ? INRANGE | INCONTACT | CONFIDENCE : CONFIDENCE;
        // Timestamps must strictly increase: a repeat is rejected with ERROR_INVALID_PARAMETER, and TickCount only
        // ticks every ~16 ms while frames arrive every ~8 ms.
        time = Math.Max(time + 1, (uint)Environment.TickCount);
        for (int i = 0; i < Fingers; i++) {
            IntPtr p = buffer + i * InfoSize;
            for (int b = 0; b < InfoSize; b += 8) Marshal.WriteInt64(p, b, 0);
            Marshal.WriteInt32(p, 0, (int)PT_TOUCHPAD);  // POINTER_TYPE_INFO.type
            Marshal.WriteInt32(p, 8, (int)PT_TOUCHPAD);  // touchInfo.pointerInfo.pointerType
            Marshal.WriteInt32(p, 12, i);                // pointerId
            Marshal.WriteInt32(p, 20, (int)flags);       // pointerFlags
            Marshal.WriteInt32(p, 48, x + FingerX[i]);   // ptHimetricLocation
            Marshal.WriteInt32(p, 52, y + FingerY[i]);
            Marshal.WriteInt32(p, 72, (int)time);         // dwTime
        }
        if (!Native.InjectSyntheticPointerInput(device, buffer, Fingers) && !failed) {
            failed = true; // once per touch, so a failing device doesn't flood the log
            Program.Log("InjectSyntheticPointerInput failed: " + Marshal.GetLastWin32Error());
        }
    }

    static int Clamp(int v, int lo, int hi) { return Math.Max(lo, Math.Min(hi, v)); }

    public void Dispose() {
        Lift();
        if (liftLater != null) liftLater.Dispose();
        if (device != IntPtr.Zero) { Native.DestroySyntheticPointerDevice(device); device = IntPtr.Zero; }
        if (buffer != IntPtr.Zero) { Marshal.FreeHGlobal(buffer); buffer = IntPtr.Zero; }
    }
}
