param(
  [string]$ProcessName = "ACU",
  [long]$Target = 0x1401DEF490,
  [int]$Hits = 5,
  [int]$Seconds = 120
)

Add-Type @"
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

public class Tracer {
  [DllImport("kernel32.dll")] public static extern IntPtr OpenProcess(uint a, bool i, int pid);
  [DllImport("kernel32.dll")] public static extern bool ReadProcessMemory(IntPtr h, IntPtr a, byte[] b, IntPtr s, out IntPtr r);
  [DllImport("kernel32.dll")] public static extern bool WriteProcessMemory(IntPtr h, IntPtr a, byte[] b, IntPtr s, out IntPtr w);
  [DllImport("kernel32.dll")] public static extern bool DebugActiveProcess(int pid);
  [DllImport("kernel32.dll")] public static extern bool DebugActiveProcessStop(int pid);
  [DllImport("kernel32.dll")] public static extern bool DebugSetProcessKillOnExit(bool kill);
  [DllImport("kernel32.dll")] public static extern bool WaitForDebugEvent(out DEBUG_EVENT e, uint ms);
  [DllImport("kernel32.dll")] public static extern bool ContinueDebugEvent(int pid, int tid, uint status);
  [DllImport("kernel32.dll")] public static extern IntPtr OpenThread(uint access, bool inherit, int tid);
  [DllImport("kernel32.dll")] public static extern bool GetThreadContext(IntPtr h, byte[] ctx);
  [DllImport("kernel32.dll")] public static extern bool SetThreadContext(IntPtr h, byte[] ctx);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);

  [StructLayout(LayoutKind.Sequential)]
  public struct DEBUG_EVENT {
    public uint code; public int pid; public int tid; public uint status;
    [MarshalAs(UnmanagedType.ByValArray, SizeConst = 160)] public byte[] body;
  }

  const uint DBG_CONTINUE = 0x00010002;
  const uint DBG_EXCEPTION_NOT_HANDLED = 0x80010001;
  const uint EXCEPTION_DEBUG_EVENT = 1;
  const uint EXCEPTION_BREAKPOINT = 0x80000003;
  const uint EXCEPTION_SINGLE_STEP = 0x80000004;

  public static void Arm(IntPtr proc, long target) {
    byte[] cc = { 0xCC };
    IntPtr w;
    WriteProcessMemory(proc, new IntPtr(target), cc, new IntPtr(1), out w);
  }

  public static string Trace(int pid, long target, int hits, int seconds) {
    var log = new StringBuilder();
    IntPtr proc = OpenProcess(0x1F0FFF, false, pid);
    if (proc == IntPtr.Zero) return "OpenProcess failed";
    byte[] original = new byte[1];
    IntPtr read;
    if (!ReadProcessMemory(proc, new IntPtr(target), original, new IntPtr(1), out read)) return "ReadProcessMemory failed";
    if (!DebugActiveProcess(pid)) return "DebugActiveProcess failed (access denied or protected)";
    DebugSetProcessKillOnExit(false);

    bool armed = false;
    Arm(proc, target);
    armed = true;

    var start = DateTime.UtcNow;
    int found = 0;
    while (found < hits && (DateTime.UtcNow - start).TotalSeconds < seconds) {
      DEBUG_EVENT ev;
      if (!WaitForDebugEvent(out ev, 1000)) continue;
      uint status = DBG_CONTINUE;
      if (ev.code == EXCEPTION_DEBUG_EVENT && ev.body != null && ev.body.Length >= 24) {
        uint code = BitConverter.ToUInt32(ev.body, 0);
        ulong addr = BitConverter.ToUInt64(ev.body, 16);
        if (code == EXCEPTION_BREAKPOINT && (long)addr == target) {
          IntPtr thread = OpenThread(0x0008 | 0x0002 /*GET|SET_CONTEXT*/, false, ev.tid);
          byte[] ctx = new byte[0x4D0];
          ulong rsp = 0, rip = 0, rcx = 0;
          if (thread != IntPtr.Zero) {
            BitConverter.GetBytes(0x0010000B).CopyTo(ctx, 0x30);
            if (GetThreadContext(thread, ctx)) {
              rsp = BitConverter.ToUInt64(ctx, 0x98);
              rip = BitConverter.ToUInt64(ctx, 0xF8);
              rcx = BitConverter.ToUInt64(ctx, 0x80);
            }
          }
          string opened = "";
          if (rcx > 0x10000UL) {
            byte[] nameBuf = new byte[520];
            IntPtr gotName;
            if (ReadProcessMemory(proc, new IntPtr((long)rcx), nameBuf, new IntPtr(nameBuf.Length), out gotName)) {
              opened = System.Text.Encoding.Unicode.GetString(nameBuf);
              int nul = opened.IndexOf('\0');
              if (nul >= 0) opened = opened.Substring(0, nul);
            }
          }
          bool want = opened.IndexOf(".forge", StringComparison.OrdinalIgnoreCase) >= 0;
          if (want) {
          log.AppendLine("HIT #" + (found + 1) + " tid=" + ev.tid + " rip=0x" + rip.ToString("x") + " rsp=0x" + rsp.ToString("x") + " file=" + opened);
          if (rsp != 0) {
            byte[] stack = new byte[8192];
            IntPtr got;
            if (ReadProcessMemory(proc, new IntPtr((long)rsp), stack, new IntPtr(stack.Length), out got)) {
              int shown = 0;
              for (int i = 0; i + 8 <= stack.Length && shown < 14; i += 8) {
                ulong value = BitConverter.ToUInt64(stack, i);
                if (value >= 0x140001000UL && value < 0x14779B000UL) {
                  log.AppendLine("   ret #" + shown + " [rsp+0x" + i.ToString("x") + "] = 0x" + value.ToString("x"));
                  shown++;
                }
              }
            }
          }
          }
          // restore, single-step past the breakpoint, then re-arm
          IntPtr w;
          WriteProcessMemory(proc, new IntPtr(target), original, new IntPtr(1), out w);
          armed = false;
          if (thread != IntPtr.Zero) {
            BitConverter.GetBytes(0x0010000B).CopyTo(ctx, 0x30);
            uint eflags = BitConverter.ToUInt32(ctx, 0x44);
            BitConverter.GetBytes(eflags | 0x100).CopyTo(ctx, 0x44);
            SetThreadContext(thread, ctx);
            CloseHandle(thread);
          }
          if (want) found++;
        } else if (code == EXCEPTION_SINGLE_STEP) {
          if (!armed) { Arm(proc, target); armed = true; }
        } else if (code == EXCEPTION_BREAKPOINT) {
          // Anything else (notably the initial ntdll breakpoint raised at attach) must be
          // swallowed by the debugger; forwarding it kills the target.
          status = DBG_CONTINUE;
        } else {
          status = DBG_EXCEPTION_NOT_HANDLED;
        }
      } else {
        status = DBG_EXCEPTION_NOT_HANDLED;
      }
      ContinueDebugEvent(ev.pid, ev.tid, status);
    }
    if (armed) {
      IntPtr w;
      WriteProcessMemory(proc, new IntPtr(target), original, new IntPtr(1), out w);
    }
    DebugActiveProcessStop(pid);
    CloseHandle(proc);
    return log.Length == 0 ? "no hits" : log.ToString();
  }
}
"@

$proc = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { Write-Error "process not running"; exit 1 }
$base = $proc.MainModule.BaseAddress.ToInt64()
if ($Target -ge 0x140000000 -and $Target -lt 0x148000000) { $Target = $base + ($Target - 0x140000000) }
Write-Output ("attaching to {0} (pid {1}) base 0x{2:x}, breakpoint 0x{3:x}" -f $ProcessName, $proc.Id, $base, $Target)
$result = [Tracer]::Trace($proc.Id, $Target, $Hits, $Seconds)
Write-Output $result
