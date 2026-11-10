param([string]$Plugin,[string]$Clip,[string]$Tag,[string]$Mode='dump',[int]$Frames=2,[int]$Skip=10,[int]$AtMs=1500,[int]$Timeout=90,[string]$Vlc='C:\nebulae-factory-hdr\vlc-base\vlc-3.0.23',[string]$Extra='')
$ErrorActionPreference='Stop'
$r='C:\nebulae-factory-hdr'
if ($Plugin) { Copy-Item "$r\plugins\$Plugin" "$Vlc\plugins\video_output\libdirect3d11_plugin.dll" -Force }
$d="$r\dumps\$Tag"
if (Test-Path $d) { Remove-Item $d -Recurse -Force }
New-Item -ItemType Directory $d | Out-Null
$clip = if ([IO.Path]::IsPathRooted($Clip)) { $Clip } else { "$r\clips\$Clip" }
$a=@('--vlc',$Vlc,'--clip',$clip,'--log',"$d\vlc.log",'--timeout-ms',"$($Timeout*1000)")
if ($Mode -eq 'dump') {
  $env:NEBULAE_D3D11_TEST_DUMP=$d; $env:NEBULAE_D3D11_TEST_DUMP_FRAMES="$Frames"; $env:NEBULAE_D3D11_TEST_DUMP_SKIP="$Skip"
  $a+=@('--dump-frames',"$Frames",'--dump-dir',$d)
} else {
  $a+=@('--capture',"$d\capture.bgra",'--at-ms',"$AtMs")
}
if ($Extra) { foreach ($e in $Extra.Split(';')) { $a+=@('--arg',$e) } }
$p=Start-Process "$r\tools\vlcdump.exe" -ArgumentList $a -PassThru -WindowStyle Hidden -RedirectStandardError "$d\harness.err"
$null=$p.Handle
if (-not $p.WaitForExit(($Timeout+30)*1000)) { $p.Kill(); "HARNESS TIMEOUT" }
"exit=$($p.ExitCode)"
Get-Content "$d\harness.err" -ErrorAction SilentlyContinue
Get-ChildItem $d | Select Name,Length | Format-Table -Auto | Out-String
Select-String -Path "$d\vlc.log" -Pattern '(colorspace|tone mapping|swapchain|SwapChain|Created the D3D11|error)' | Select -First 12 | ForEach-Object { $_.Line }
