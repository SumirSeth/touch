using System;
using System.Runtime.InteropServices;

static class Native {
    [StructLayout(LayoutKind.Sequential)] public struct RAWINPUTDEVICE { public ushort Page, Usage; public uint Flags; public IntPtr Target; }
    [DllImport("user32.dll", SetLastError = true)] public static extern bool RegisterRawInputDevices(RAWINPUTDEVICE[] d, uint n, uint sz);
    [DllImport("user32.dll")] public static extern uint GetRawInputData(IntPtr h, uint cmd, IntPtr data, ref uint size, uint hdr);
    [DllImport("user32.dll")] public static extern uint GetRawInputDeviceInfo(IntPtr dev, uint cmd, IntPtr data, ref uint size);
    [DllImport("user32.dll", EntryPoint = "GetRawInputDeviceInfoW", CharSet = CharSet.Unicode)] public static extern uint GetRawInputDeviceName(IntPtr dev, uint cmd, System.Text.StringBuilder name, ref uint chars);
    [DllImport("hid.dll")] public static extern int HidP_GetUsageValue(int type, ushort page, ushort link, ushort usage, out uint value, IntPtr pp, IntPtr report, uint len);
    [DllImport("hid.dll")] public static extern int HidP_GetUsages(int type, ushort page, ushort link, ushort[] list, ref uint len, IntPtr pp, IntPtr report, uint len2);
    [DllImport("hid.dll")] public static extern int HidP_GetSpecificValueCaps(int type, ushort page, ushort link, ushort usage, byte[] caps, ref ushort len, IntPtr pp);
    public const int HIDP_STATUS_SUCCESS = 0x00110000;

    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
    [DllImport("user32.dll")] public static extern bool ClipCursor(ref RECT r);
    [DllImport("user32.dll", EntryPoint = "ClipCursor")] public static extern bool UnclipCursor(IntPtr none);
    [DllImport("kernel32.dll")] public static extern bool AllocConsole();

    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, System.Text.StringBuilder name, int max);
    public static string ForegroundClass() {
        var name = new System.Text.StringBuilder(256);
        GetClassName(GetForegroundWindow(), name, name.Capacity);
        return name.ToString();
    }

    // Synthetic pointer devices (CreateSyntheticPointerDevice2 is needed for PT_TOUCHPAD).
    [StructLayout(LayoutKind.Sequential)]
    public struct SYNTHETIC_DEVICE_CREATION_PARAMS { public uint PointerType, MaxCount, FeedbackMode; public IntPtr Monitor; public uint Width, Height, Options; }
    [DllImport("user32.dll", SetLastError = true)] public static extern IntPtr CreateSyntheticPointerDevice2(ref SYNTHETIC_DEVICE_CREATION_PARAMS p);
    [DllImport("user32.dll")] public static extern void DestroySyntheticPointerDevice(IntPtr device);
    [DllImport("user32.dll", SetLastError = true)] public static extern bool InjectSyntheticPointerInput(IntPtr device, IntPtr infos, uint count);

    [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort Vk, Scan; public uint Flags, Time; public IntPtr Extra; }
    [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int Dx, Dy; public uint Data, Flags, Time; public IntPtr Extra; }
    // INPUT is a union; on x64 the union starts at offset 8 and the struct is 40 bytes.
    [StructLayout(LayoutKind.Explicit, Size = 40)] struct INPUT {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public MOUSEINPUT Mi;
        [FieldOffset(8)] public KEYBDINPUT Ki;
    }
    [DllImport("user32.dll", SetLastError = true)] static extern uint SendInput(uint n, INPUT[] inputs, int size);

    static void Send(INPUT[] inputs) { SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(INPUT))); }

    // Presses the keys in order, then releases them in reverse.
    public static void SendChord(params ushort[] vks) {
        var inputs = new INPUT[vks.Length * 2];
        for (int i = 0; i < vks.Length; i++) {
            inputs[i].Type = 1; inputs[i].Ki.Vk = vks[i];
            var up = vks.Length * 2 - 1 - i;
            inputs[up].Type = 1; inputs[up].Ki.Vk = vks[i]; inputs[up].Ki.Flags = 2; // KEYEVENTF_KEYUP
        }
        Send(inputs);
    }

    public static void SendMouseMove(int dx, int dy) {
        var i = new INPUT[1];
        i[0].Mi.Dx = dx; i[0].Mi.Dy = dy; i[0].Mi.Flags = 0x1; // MOUSEEVENTF_MOVE
        Send(i);
    }

    public static void SendLeftButton(bool down) {
        var i = new INPUT[1];
        i[0].Mi.Flags = down ? 0x2u : 0x4u; // MOUSEEVENTF_LEFTDOWN / LEFTUP
        Send(i);
    }
}
