using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Windows.Forms;

struct Contact { public uint Id; public bool Tip; public double X, Y; } // X/Y in mm from the top-left corner

// One complete scan of the touchpad: every finger currently touching.
class Frame {
    public List<Contact> Touching;
    public double Width, Height; // pad size in mm
    public int Time;             // milliseconds (high-resolution clock) when the frame completed
    public bool Button;          // the pad is pressed down (clicked)

    public static int Now() { return (int)(System.Diagnostics.Stopwatch.GetTimestamp() * 1000 / System.Diagnostics.Stopwatch.Frequency); }
}

// Receives raw Precision Touchpad reports via Raw Input (alongside Windows' own handling) and reassembles them into frames.
// This touchpad uses hybrid reporting: one contact per report, and the first report of a frame carries the total count.
class TouchpadSink : NativeWindow {
    const int WM_INPUT = 0xFF;
    const uint RID_INPUT = 0x10000003, RIDI_PREPARSEDDATA = 0x20000005, RIDI_DEVICENAME = 0x20000007, RIDEV_INPUTSINK = 0x100;

    class Device { public IntPtr Preparsed; public double MmPerX, MmPerY, Width, Height; public bool Synthetic; }

    public event Action<Frame> FrameReady;
    readonly Dictionary<IntPtr, Device> devices = new Dictionary<IntPtr, Device>();
    readonly List<Contact> pending = new List<Contact>();
    uint expected;
    bool button;

    public TouchpadSink() {
        CreateHandle(new CreateParams());
        var d = new[] { new Native.RAWINPUTDEVICE { Page = 0x0D, Usage = 0x05, Flags = RIDEV_INPUTSINK, Target = Handle } };
        if (!Native.RegisterRawInputDevices(d, 1, (uint)Marshal.SizeOf(typeof(Native.RAWINPUTDEVICE))))
            throw new Exception("RegisterRawInputDevices failed: " + Marshal.GetLastWin32Error());
    }

    Device Lookup(IntPtr handle) {
        Device dev;
        if (devices.TryGetValue(handle, out dev)) return dev;
        // Synthetic touchpads (including our own VirtualTouchpad) also produce raw input. Reading our own injected
        // fingers back would look like a 4th finger landing and cancel the swipe being replayed, so ignore them.
        var name = new System.Text.StringBuilder(512); uint chars = (uint)name.Capacity;
        Native.GetRawInputDeviceName(handle, RIDI_DEVICENAME, name, ref chars);
        if (name.ToString().StartsWith(@"\\?\Microsoft HID RID", StringComparison.OrdinalIgnoreCase))
            return devices[handle] = new Device { Synthetic = true };
        uint size = 0; Native.GetRawInputDeviceInfo(handle, RIDI_PREPARSEDDATA, IntPtr.Zero, ref size);
        dev = new Device { Preparsed = Marshal.AllocHGlobal((int)size) };
        Native.GetRawInputDeviceInfo(handle, RIDI_PREPARSEDDATA, dev.Preparsed, ref size);
        dev.Width = Axis(dev.Preparsed, 0x30, out dev.MmPerX);
        dev.Height = Axis(dev.Preparsed, 0x31, out dev.MmPerY);
        devices[handle] = dev;
        return dev;
    }

    // Returns the axis length in mm and the mm per logical unit, from the descriptor's physical extent (cm or inch units).
    static double Axis(IntPtr pp, ushort usage, out double mmPerUnit) {
        mmPerUnit = 1;
        var caps = new byte[72]; ushort n = 1;
        if (Native.HidP_GetSpecificValueCaps(0, 0x01, 1, usage, caps, ref n, pp) != Native.HIDP_STATUS_SUCCESS || n == 0) return 100;
        uint units = BitConverter.ToUInt32(caps, 36), exp = BitConverter.ToUInt32(caps, 32);
        int logical = BitConverter.ToInt32(caps, 44) - BitConverter.ToInt32(caps, 40);
        int physical = BitConverter.ToInt32(caps, 52) - BitConverter.ToInt32(caps, 48);
        if (logical <= 0 || physical <= 0) return logical > 0 ? logical : 100;
        int e = (int)(exp & 0xF); if (e > 7) e -= 16;
        double mm = physical * Math.Pow(10, e) * ((units & 0xF) == 3 ? 25.4 : 10); // 0x13 = inch, 0x11 = cm
        mmPerUnit = mm / logical;
        return mm;
    }

    static uint Value(ushort page, ushort link, ushort usage, IntPtr pp, IntPtr rep, uint len) {
        uint v; return Native.HidP_GetUsageValue(0, page, link, usage, out v, pp, rep, len) == Native.HIDP_STATUS_SUCCESS ? v : 0;
    }

    protected override void WndProc(ref Message m) {
        if (m.Msg == WM_INPUT) {
            uint hdr = (uint)(IntPtr.Size == 8 ? 24 : 16); // RAWINPUTHEADER
            uint size = 0; Native.GetRawInputData(m.LParam, RID_INPUT, IntPtr.Zero, ref size, hdr);
            IntPtr buf = Marshal.AllocHGlobal((int)size);
            try {
                Native.GetRawInputData(m.LParam, RID_INPUT, buf, ref size, hdr);
                var dev = Lookup(Marshal.ReadIntPtr(buf, 8));
                uint repSize = (uint)Marshal.ReadInt32(buf, (int)hdr), count = (uint)Marshal.ReadInt32(buf, (int)hdr + 4);
                for (uint i = 0; i < count && !dev.Synthetic; i++)
                    OnReport(buf + (int)hdr + 8 + (int)(i * repSize), repSize, dev);
            } finally { Marshal.FreeHGlobal(buf); }
        }
        base.WndProc(ref m);
    }

    void OnReport(IntPtr rep, uint len, Device dev) {
        IntPtr pp = dev.Preparsed;
        uint count = Value(0x0D, 0, 0x54, pp, rep, len);
        if (count > 0) { pending.Clear(); expected = count; button = false; }
        else if (expected == 0) return; // continuation report without a frame start; resync on the next one

        var usages = new ushort[8]; uint n = 8;
        Native.HidP_GetUsages(0, 0x0D, 1, usages, ref n, pp, rep, len);
        var buttons = new ushort[4]; uint nb = 4;
        if (Native.HidP_GetUsages(0, 0x09, 0, buttons, ref nb, pp, rep, len) == Native.HIDP_STATUS_SUCCESS && nb > 0) button = true;
        pending.Add(new Contact {
            Id = Value(0x0D, 1, 0x51, pp, rep, len),
            Tip = Array.IndexOf(usages, (ushort)0x42, 0, (int)n) >= 0,
            X = Value(0x01, 1, 0x30, pp, rep, len) * dev.MmPerX,
            Y = Value(0x01, 1, 0x31, pp, rep, len) * dev.MmPerY,
        });
        if (pending.Count < expected) return;

        expected = 0;
        var frame = new Frame { Touching = pending.FindAll(c => c.Tip), Width = dev.Width, Height = dev.Height, Time = Frame.Now(), Button = button };
        pending.Clear();
        if (FrameReady != null) FrameReady(frame);
    }
}
