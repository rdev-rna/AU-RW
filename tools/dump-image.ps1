# Dump the unpacked image of a running process to a flat file at its module base.
# Usage: dump-image.ps1 -ProcessName ACU -Out local\dump\ACU_dump.bin
param(
  [string]$ProcessName = "ACU",
  [string]$Out = "local\dump\ACU_dump.bin",
  [int]$WaitSeconds = 40
)

Add-Type @"
using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;

public class MemDump {
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool ReadProcessMemory(IntPtr h, IntPtr addr, byte[] buffer, IntPtr size, out IntPtr read);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool CloseHandle(IntPtr h);

  [StructLayout(LayoutKind.Sequential)]
  public struct MEMORY_BASIC_INFORMATION {
    public IntPtr BaseAddress;
    public IntPtr AllocationBase;
    public uint AllocationProtect;
    public IntPtr RegionSize;
    public uint State;
    public uint Protect;
    public uint Type;
  }

  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern IntPtr VirtualQueryEx(IntPtr h, IntPtr addr, out MEMORY_BASIC_INFORMATION info, IntPtr size);

  public static long Dump(int pid, long baseAddr, long size, string path) {
    const uint PROCESS_QUERY_INFORMATION = 0x0400;
    const uint PROCESS_VM_READ = 0x0010;
    IntPtr h = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid);
    if (h == IntPtr.Zero) return -1;
    long written = 0;
    try {
      using (FileStream fs = new FileStream(path, FileMode.Create, FileAccess.Write)) {
        fs.SetLength(size);
        long cursor = baseAddr;
        long end = baseAddr + size;
        while (cursor < end) {
          MEMORY_BASIC_INFORMATION info;
          IntPtr got = VirtualQueryEx(h, new IntPtr(cursor), out info, new IntPtr(Marshal.SizeOf(typeof(MEMORY_BASIC_INFORMATION))));
          if (got == IntPtr.Zero) break;
          long regionSize = info.RegionSize.ToInt64();
          bool readable = info.State == 0x1000 && (info.Protect & 0x100) == 0 && (info.Protect & 0x01) == 0;
          if (readable) {
            long regionBase = Math.Max(cursor, info.BaseAddress.ToInt64());
            long regionEnd = Math.Min(end, info.BaseAddress.ToInt64() + regionSize);
            long length = regionEnd - regionBase;
            if (length > 0) {
              byte[] buffer = new byte[length];
              IntPtr read;
              if (ReadProcessMemory(h, new IntPtr(regionBase), buffer, new IntPtr(length), out read)) {
                fs.Position = regionBase - baseAddr;
                fs.Write(buffer, 0, (int)read.ToInt64());
                written += read.ToInt64();
              }
            }
          }
          cursor = info.BaseAddress.ToInt64() + regionSize;
          if (regionSize <= 0) break;
        }
      }
    } finally {
      CloseHandle(h);
    }
    return written;
  }
}
"@

$proc = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) {
  $exe = "C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity\ACU.exe"
  Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe) | Out-Null
  Write-Output "launched, waiting $WaitSeconds s for unpack"
  Start-Sleep -Seconds $WaitSeconds
  $proc = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Select-Object -First 1
}
if (-not $proc) { Write-Error "process not found"; exit 1 }

$proc.Refresh()
$base = $proc.MainModule.BaseAddress.ToInt64()
$size = $proc.MainModule.ModuleMemorySize
Write-Output ("pid={0} base=0x{1:x} size=0x{2:x} ({3} MB)" -f $proc.Id, $base, $size, [math]::Round($size/1MB))

$path = Join-Path (Get-Location) $Out
$dir = Split-Path -Parent $path
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$written = [MemDump]::Dump($proc.Id, $base, $size, $path)
if ($written -lt 0) { Write-Error "OpenProcess failed (access denied?)"; exit 2 }
Write-Output ("dumped {0} MB to {1}" -f [math]::Round($written/1MB,1), $path)
