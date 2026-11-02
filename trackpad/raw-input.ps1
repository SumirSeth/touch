# Streams raw Precision Touchpad contacts (HID usage page 0x0D / usage 0x05) via the Win32 Raw Input API.
# Runs alongside Windows' own touchpad handling (RIDEV_INPUTSINK), so the cursor keeps working while this prints.
param([int]$Seconds = 20)

Add-Type -ReferencedAssemblies System.Windows.Forms -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Windows.Forms;

public class PtpSink : NativeWindow {
    [StructLayout(LayoutKind.Sequential)] struct RAWINPUTDEVICE { public ushort Page, Usage; public uint Flags; public IntPtr Target; }
    [DllImport("user32.dll")] static extern bool RegisterRawInputDevices(RAWINPUTDEVICE[] d, uint n, uint sz);
    [DllImport("user32.dll")] static extern uint GetRawInputData(IntPtr h, uint cmd, IntPtr data, ref uint size, uint hdr);
    [DllImport("user32.dll")] static extern uint GetRawInputDeviceInfo(IntPtr dev, uint cmd, IntPtr data, ref uint size);
    [DllImport("hid.dll")] static extern int HidP_GetUsageValue(int type, ushort page, ushort link, ushort usage, out uint value, IntPtr pp, IntPtr report, uint len);
    [DllImport("hid.dll")] static extern int HidP_GetUsages(int type, ushort page, ushort link, ushort[] list, ref uint len, IntPtr pp, IntPtr report, uint len2);

    const int WM_INPUT = 0xFF, HIDP_STATUS_SUCCESS = 0x00110000;
    const uint RID_INPUT = 0x10000003, RIDI_PREPARSEDDATA = 0x20000005, RIDEV_INPUTSINK = 0x100;
    public int Reports;

    public PtpSink() {
        CreateHandle(new CreateParams());
        var d = new[] { new RAWINPUTDEVICE { Page = 0x0D, Usage = 0x05, Flags = RIDEV_INPUTSINK, Target = Handle } };
        if (!RegisterRawInputDevices(d, 1, (uint)Marshal.SizeOf(typeof(RAWINPUTDEVICE))))
            throw new Exception("RegisterRawInputDevices failed: " + Marshal.GetLastWin32Error());
    }

    uint Value(ushort page, ushort link, ushort usage, IntPtr pp, IntPtr rep, uint len) {
        uint v; return HidP_GetUsageValue(0, page, link, usage, out v, pp, rep, len) == HIDP_STATUS_SUCCESS ? v : uint.MaxValue;
    }

    protected override void WndProc(ref Message m) {
        if (m.Msg == WM_INPUT) {
            int hdr = IntPtr.Size == 8 ? 24 : 16;  // RAWINPUTHEADER
            uint size = 0; GetRawInputData(m.LParam, RID_INPUT, IntPtr.Zero, ref size, (uint)hdr);
            IntPtr buf = Marshal.AllocHGlobal((int)size);
            try {
                GetRawInputData(m.LParam, RID_INPUT, buf, ref size, (uint)hdr);
                IntPtr dev = Marshal.ReadIntPtr(buf, 8);
                uint ppSize = 0; GetRawInputDeviceInfo(dev, RIDI_PREPARSEDDATA, IntPtr.Zero, ref ppSize);
                IntPtr pp = Marshal.AllocHGlobal((int)ppSize);
                GetRawInputDeviceInfo(dev, RIDI_PREPARSEDDATA, pp, ref ppSize);
                // RAWHID: dwSizeHid, dwCount, bRawData[]
                uint repSize = (uint)Marshal.ReadInt32(buf, hdr), count = (uint)Marshal.ReadInt32(buf, hdr + 4);
                for (uint i = 0; i < count; i++) {
                    IntPtr rep = buf + hdr + 8 + (int)(i * repSize);
                    Reports++;
                    ushort[] usages = new ushort[8]; uint n = 8;
                    HidP_GetUsages(0, 0x0D, 1, usages, ref n, pp, rep, repSize);
                    bool tip = false, conf = false;
                    for (int u = 0; u < n; u++) { if (usages[u] == 0x42) tip = true; if (usages[u] == 0x47) conf = true; }
                    uint btnN = 8; ushort[] btn = new ushort[8];
                    HidP_GetUsages(0, 0x09, 0, btn, ref btnN, pp, rep, repSize);
                    Console.WriteLine("scan={0,5} contacts={1} | id={2} tip={3} conf={4} x={5,4} y={6,4} | click={7}",
                        Value(0x0D, 0, 0x56, pp, rep, repSize), Value(0x0D, 0, 0x54, pp, rep, repSize),
                        Value(0x0D, 1, 0x51, pp, rep, repSize), tip ? 1 : 0, conf ? 1 : 0,
                        Value(0x01, 1, 0x30, pp, rep, repSize), Value(0x01, 1, 0x31, pp, rep, repSize), btnN > 0 ? 1 : 0);
                }
                Marshal.FreeHGlobal(pp);
            } finally { Marshal.FreeHGlobal(buf); }
        }
        base.WndProc(ref m);
    }
}
'@

$sink = New-Object PtpSink
Write-Host "Listening for touchpad input for $Seconds s - touch the trackpad..."
$end = (Get-Date).AddSeconds($Seconds)
while ((Get-Date) -lt $end) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 5 }
Write-Host "Received $($sink.Reports) contact reports."
$sink.DestroyHandle()
