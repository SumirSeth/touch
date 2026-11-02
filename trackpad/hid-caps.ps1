# Dumps the HID capabilities (parsed report descriptor) of every touchpad collection.
# Read-only: opens each HID interface with zero access rights, which is enough for HidD_GetPreparsedData.
param([string]$Match = 'asue1306')

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
using Microsoft.Win32.SafeHandles;

public static class Hid {
    [StructLayout(LayoutKind.Sequential)] public struct CAPS {
        public ushort Usage, UsagePage, InputReportByteLength, OutputReportByteLength, FeatureReportByteLength;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 17)] public ushort[] Reserved;
        public ushort NumberLinkCollectionNodes, NumberInputButtonCaps, NumberInputValueCaps, NumberInputDataIndices,
            NumberOutputButtonCaps, NumberOutputValueCaps, NumberOutputDataIndices,
            NumberFeatureButtonCaps, NumberFeatureValueCaps, NumberFeatureDataIndices;
    }
    // HIDP_VALUE_CAPS / HIDP_BUTTON_CAPS share a 72-byte layout; we decode the fields we need by offset.
    [StructLayout(LayoutKind.Sequential)] struct SP_DEVICE_INTERFACE_DATA { public int cbSize; public Guid g; public int f; public IntPtr r; }

    [DllImport("hid.dll")] static extern void HidD_GetHidGuid(out Guid g);
    [DllImport("hid.dll")] static extern bool HidD_GetPreparsedData(SafeFileHandle h, out IntPtr p);
    [DllImport("hid.dll")] static extern bool HidD_FreePreparsedData(IntPtr p);
    [DllImport("hid.dll")] static extern int HidP_GetCaps(IntPtr p, out CAPS c);
    [DllImport("hid.dll")] static extern int HidP_GetValueCaps(int type, byte[] caps, ref ushort len, IntPtr p);
    [DllImport("hid.dll")] static extern int HidP_GetButtonCaps(int type, byte[] caps, ref ushort len, IntPtr p);
    [DllImport("setupapi.dll", CharSet = CharSet.Unicode)] static extern IntPtr SetupDiGetClassDevs(ref Guid g, IntPtr e, IntPtr h, int f);
    [DllImport("setupapi.dll")] static extern bool SetupDiEnumDeviceInterfaces(IntPtr s, IntPtr d, ref Guid g, int i, ref SP_DEVICE_INTERFACE_DATA o);
    [DllImport("setupapi.dll", CharSet = CharSet.Unicode)] static extern bool SetupDiGetDeviceInterfaceDetail(IntPtr s, ref SP_DEVICE_INTERFACE_DATA d, IntPtr b, int sz, out int req, IntPtr di);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern SafeFileHandle CreateFile(string n, uint a, uint s, IntPtr sa, uint c, uint f, IntPtr t);

    public static List<string> Paths() {
        Guid g; HidD_GetHidGuid(out g);
        IntPtr set = SetupDiGetClassDevs(ref g, IntPtr.Zero, IntPtr.Zero, 0x12);
        var list = new List<string>();
        var d = new SP_DEVICE_INTERFACE_DATA(); d.cbSize = Marshal.SizeOf(d);
        for (int i = 0; SetupDiEnumDeviceInterfaces(set, IntPtr.Zero, ref g, i, ref d); i++) {
            int req; SetupDiGetDeviceInterfaceDetail(set, ref d, IntPtr.Zero, 0, out req, IntPtr.Zero);
            IntPtr buf = Marshal.AllocHGlobal(req);
            Marshal.WriteInt32(buf, 8);
            if (SetupDiGetDeviceInterfaceDetail(set, ref d, buf, req, out req, IntPtr.Zero))
                list.Add(Marshal.PtrToStringUni(buf + 4));
            Marshal.FreeHGlobal(buf);
        }
        return list;
    }

    public static string Describe(string path) {
        var sb = new System.Text.StringBuilder();
        using (var h = CreateFile(path, 0, 3, IntPtr.Zero, 3, 0, IntPtr.Zero)) {
            if (h.IsInvalid) return "  (cannot open, error " + Marshal.GetLastWin32Error() + ")\n";
            IntPtr p; if (!HidD_GetPreparsedData(h, out p)) return "  (no preparsed data)\n";
            CAPS c; HidP_GetCaps(p, out c);
            sb.AppendFormat("  TLC UsagePage 0x{0:X2} Usage 0x{1:X2} | report bytes in={2} out={3} feature={4} | link nodes={5}\n",
                c.UsagePage, c.Usage, c.InputReportByteLength, c.OutputReportByteLength, c.FeatureReportByteLength, c.NumberLinkCollectionNodes);
            string[] kinds = { "Input", "Output", "Feature" };
            ushort[] nb = { c.NumberInputButtonCaps, c.NumberOutputButtonCaps, c.NumberFeatureButtonCaps };
            ushort[] nv = { c.NumberInputValueCaps, c.NumberOutputValueCaps, c.NumberFeatureValueCaps };
            for (int t = 0; t < 3; t++) {
                if (nb[t] > 0) {
                    ushort n = nb[t]; var b = new byte[72 * n]; HidP_GetButtonCaps(t, b, ref n, p);
                    for (int i = 0; i < n; i++) {
                        int o = i * 72; bool range = b[o + 12] != 0;
                        sb.AppendFormat("    {0,-7} button  report 0x{1:X2} page 0x{2:X2} usage {3} link {4}\n", kinds[t], b[o + 2], BitConverter.ToUInt16(b, o),
                            range ? string.Format("0x{0:X2}-0x{1:X2}", BitConverter.ToUInt16(b, o + 56), BitConverter.ToUInt16(b, o + 58)) : string.Format("0x{0:X2}", BitConverter.ToUInt16(b, o + 56)),
                            BitConverter.ToUInt16(b, o + 6));
                    }
                }
                if (nv[t] > 0) {
                    ushort n = nv[t]; var b = new byte[72 * n]; HidP_GetValueCaps(t, b, ref n, p);
                    for (int i = 0; i < n; i++) {
                        int o = i * 72;
                        sb.AppendFormat("    {0,-7} value   report 0x{1:X2} page 0x{2:X2} usage 0x{3:X2} link {4} bits {5}x{6} logical [{7},{8}] physical [{9},{10}] unit 0x{11:X} exp {12}\n",
                            kinds[t], b[o + 2], BitConverter.ToUInt16(b, o), BitConverter.ToUInt16(b, o + 56), BitConverter.ToUInt16(b, o + 6),
                            BitConverter.ToUInt16(b, o + 18), BitConverter.ToUInt16(b, o + 20),
                            BitConverter.ToInt32(b, o + 40), BitConverter.ToInt32(b, o + 44), BitConverter.ToInt32(b, o + 48), BitConverter.ToInt32(b, o + 52),
                            BitConverter.ToUInt32(b, o + 36), BitConverter.ToUInt32(b, o + 32));
                    }
                }
            }
            HidD_FreePreparsedData(p);
        }
        return sb.ToString();
    }
}
'@

foreach ($path in [Hid]::Paths() | Where-Object { $_ -match $Match }) {
    $path
    [Hid]::Describe($path)
}
