# M0 memory sampler — THROWAWAY. Sums the spike-m0 process tree (main + WebView2
# children) once a second to a CSV, because destroy() frees the child processes,
# not the host. Authoritative record for T1-T4; Task Manager is the human cross-check.
param(
  [int]$Seconds = 240,
  [string]$Name = "spike-m0",
  [string]$Out = "$PSScriptRoot\m0-measure.csv"
)

function Get-Tree($rootId) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
  $result = @($rootId)
  $frontier = @($rootId)
  while ($frontier.Count) {
    $kids = $all | Where-Object { $frontier -contains $_.ParentProcessId } |
            Select-Object -ExpandProperty ProcessId |
            Where-Object { $result -notcontains $_ }
    if (-not $kids) { break }
    $result += $kids
    $frontier = $kids
  }
  $result
}

"timestamp,proc_count,working_set_mb,private_mb" | Out-File $Out -Encoding utf8
Write-Output "sampling '$Name' process tree for ${Seconds}s -> $Out"
$end = (Get-Date).AddSeconds($Seconds)
while ((Get-Date) -lt $end) {
  $main = Get-Process -Name $Name -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($main) {
    $ids = Get-Tree $main.Id
    $procs = Get-Process -Id $ids -ErrorAction SilentlyContinue
    $ws = [math]::Round((($procs | Measure-Object WorkingSet64 -Sum).Sum) / 1MB, 1)
    $pv = [math]::Round((($procs | Measure-Object PrivateMemorySize64 -Sum).Sum) / 1MB, 1)
    $line = "{0:HH:mm:ss},{1},{2},{3}" -f (Get-Date), $procs.Count, $ws, $pv
  } else {
    $line = "{0:HH:mm:ss},0,," -f (Get-Date)
  }
  Write-Output $line
  $line | Out-File $Out -Append -Encoding utf8
  Start-Sleep -Seconds 1
}
Write-Output "done -> $Out"
