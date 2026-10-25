param([string]$ProcessName = "ACU", [string]$PatternHex = "90f4de0140010000", [int]$MaxHits = 6)

Add-Type @"
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;

public class MemScan {
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool ReadProcessMemory(IntPtr h, IntPtr addr, byte[] buf, IntPtr size, out IntPtr read);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool CloseHandle(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct MEMORY_BASIC_INFORMATION {
    public IntPtr BaseAddress; public IntPtr AllocationBase; public uint AllocationProtect;
    public IntPtr RegionSize; public uint State; public uint Protect; public uint Type;
  }
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr VirtualQueryEx(IntPtr h, IntPtr addr, out MEMORY_BASIC_INFORMATION info, IntPtr size);

  public static List<long> Scan(int pid, byte[] needle, int maxHits) {
    var hits = new List<long>();
    IntPtr h = OpenProcess(0x0400 | 0x0010, false, pid);
    if (h == IntPtr.Zero) throw new Exception("OpenProcess denied");
    try {
      long cursor = 0x10000;
      long max = 0x7FFFFFFF0000L;
      int chunk = 4 * 1024 * 1024;
      byte[] buf = new byte[chunk + needle.Length];
      while (cursor < max && hits.Count < maxHits) {
        MEMORY_BASIC_INFORMATION info;
        if (VirtualQueryEx(h, new IntPtr(cursor), out info, new IntPtr(Marshal.SizeOf(typeof(MEMORY_BASIC_INFORMATION)))) == IntPtr.Zero) break;
        long regionBase = info.BaseAddress.ToInt64();
        long regionSize = info.RegionSize.ToInt64();
        bool readable = info.State == 0x1000 && (info.Protect & 0x100) == 0 && (info.Protect & 0x01) == 0;
        if (readable && regionSize > 0) {
          long at = regionBase;
          long end = regionBase + regionSize;
          while (at < end && hits.Count < maxHits) {
            int want = (int)Math.Min(chunk, end - at);
            IntPtr read;
            if (ReadProcessMemory(h, new IntPtr(at), buf, new IntPtr(want), out read)) {
              int n = (int)read.ToInt64();
              for (int i = 0; i + needle.Length <= n; i++) {
                bool ok = true;
                for (int k = 0; k < needle.Length; k++) { if (buf[i + k] != needle[k]) { ok = false; break; } }
                if (ok) { hits.Add(at + i); if (hits.Count >= maxHits) break; }
              }
            }
            at += want;
          }
        }
        cursor = regionBase + regionSize;
        if (regionSize <= 0) cursor += 0x1000;
      }
    } finally { CloseHandle(h); }
    return hits;
  }

  public static byte[] Read(int pid, long addr, int size) {
    IntPtr h = OpenProcess(0x0400 | 0x0010, false, pid);
    if (h == IntPtr.Zero) return null;
    try {
      byte[] buf = new byte[size];
      IntPtr read;
      if (!ReadProcessMemory(h, new IntPtr(addr), buf, new IntPtr(size), out read)) return null;
      return buf;
    } finally { CloseHandle(h); }
  }
}
"@

$proc = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { Write-Error "process not running"; exit 1 }
$needle = [byte[]]::new($PatternHex.Length / 2)
for ($i = 0; $i -lt $needle.Length; $i++) { $needle[$i] = [Convert]::ToByte($PatternHex.Substring($i*2, 2), 16) }

$hits = [MemScan]::Scan($proc.Id, $needle, $MaxHits)
Write-Output ("pattern {0}: {1} hits" -f $PatternHex, $hits.Count)
foreach ($h in $hits) {
  Write-Output ("  hit at 0x{0:x}" -f $h)
  $ctx = [MemScan]::Read($proc.Id, $h - 64, 160)
  if ($ctx) {
    $line = ($ctx | ForEach-Object { $_.ToString("x2") }) -join " "
    Write-Output ("    ctx: " + $line)
  }
}
