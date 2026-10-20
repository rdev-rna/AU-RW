param([Parameter(Mandatory=$true)][int]$ProcessId,[Parameter(Mandatory=$true)][string]$Command)
$ErrorActionPreference='Stop'
$pipe = [System.IO.Pipes.NamedPipeClientStream]::new('.', "acu-probe-$ProcessId", [System.IO.Pipes.PipeDirection]::InOut)
try {
    $pipe.Connect(5000)
    $bytes=[Text.Encoding]::UTF8.GetBytes($Command + "`n")
    $pipe.Write($bytes,0,$bytes.Length)
    $pipe.Flush()
    $reader=[IO.StreamReader]::new($pipe,[Text.Encoding]::UTF8,$false,4096,$true)
    $result=$reader.ReadLine()
    $ack=[Text.Encoding]::ASCII.GetBytes("ACK")
    $pipe.Write($ack,0,$ack.Length)
    $result
} finally { $pipe.Dispose() }

