using System;
using System.Management;
using System.Threading;

// Adjusts the built-in display's brightness through WMI. WMI calls take tens of milliseconds, so changes
// requested faster than that are coalesced on a background thread.
static class Brightness {
    static readonly object gate = new object();
    static int pending, current = -1;
    static bool busy;

    // Re-read the real brightness before the next change (it may have been changed with the keyboard).
    public static void Resync() { lock (gate) current = -1; }

    public static void Adjust(int delta) {
        lock (gate) {
            pending += delta;
            if (busy) return;
            busy = true;
        }
        ThreadPool.QueueUserWorkItem(_ => Drain());
    }

    static void Drain() {
        while (true) {
            int delta, value;
            lock (gate) {
                delta = pending; pending = 0;
                if (delta == 0) { busy = false; return; }
                value = current;
            }
            try {
                if (value < 0) value = Read();
                value = Math.Max(0, Math.Min(100, value + delta));
                Write(value);
                lock (gate) current = value;
            } catch (Exception) {
                Resync(); // e.g. an external monitor without WMI brightness support
            }
        }
    }

    static int Read() {
        using (var s = new ManagementObjectSearcher(@"root\WMI", "SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active=TRUE"))
            foreach (ManagementObject o in s.Get()) return Convert.ToInt32(o["CurrentBrightness"]);
        throw new InvalidOperationException("No display supports WMI brightness.");
    }

    static void Write(int value) {
        using (var c = new ManagementClass(@"root\WMI", "WmiMonitorBrightnessMethods", null))
            foreach (ManagementObject o in c.GetInstances()) o.InvokeMethod("WmiSetBrightness", new object[] { (uint)1, (byte)value });
    }
}
