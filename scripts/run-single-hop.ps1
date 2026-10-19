param(
    [string]$LogFile = "C:\Users\administrator\hop.log",
    [switch]$CheckUpToDate
)

Remove-Item -Path $LogFile -ErrorAction SilentlyContinue
Remove-Item -Path "$LogFile.err" -ErrorAction SilentlyContinue

$env:RUST_LOG = "info,nebulae_torrent_client_lib::updater=debug,tauri_plugin_updater=debug"
$env:NEBULAE_UPDATER_TEST_ENDPOINT = "https://asiandvdclub.org/app-update-test-a7f92e4c91b5/latest.json"
$env:NEBULAE_UPDATER_TEST_QUIT_ON_STAGED = "0"
Remove-Item env:\NEBULAE_TEST_CDP -ErrorAction SilentlyContinue
Remove-Item env:\NEBULAE_UPDATER_TEST_PUBKEY -ErrorAction SilentlyContinue

$appPath = "C:\Users\administrator\AppData\Local\Nebulae\nebulae.exe"

$p = Start-Process -FilePath $appPath -RedirectStandardOutput $LogFile -RedirectStandardError "$LogFile.err" -PassThru
Write-Host "Started nebulae.exe (PID $($p.Id))..."

if ($CheckUpToDate) {
    Write-Host "Monitoring for 'up to date' check..."
    for ($i = 0; $i -lt 15; $i++) {
        Start-Sleep -Seconds 1
        if (Test-Path "$LogFile.err") {
            $content = Get-Content "$LogFile.err" -ErrorAction SilentlyContinue | Out-String
            if ($content -match "Nebulae is up to date") {
                Write-Host "Discovered: Nebulae is up to date; no update available."
                break
            }
        }
    }
    Start-Sleep -Seconds 1
    $p.CloseMainWindow() | Out-Null
} else {
    Write-Host "Monitoring for update staging..."
    $staged = $false
    for ($i = 0; $i -lt 45; $i++) {
        Start-Sleep -Seconds 1
        if (Test-Path "$LogFile.err") {
            $content = Get-Content "$LogFile.err" -ErrorAction SilentlyContinue | Out-String
            if ($content -match "Silent update staged in memory") {
                $staged = $true
                Write-Host "Detected update staged in memory! Executing natural quit via CloseMainWindow()..."
                Start-Sleep -Milliseconds 500
                $p.CloseMainWindow() | Out-Null
                break
            }
        }
    }

    if (-not $staged) {
        Write-Host "WARNING: Staging event not detected within 45s. Closing main window..."
        $p.CloseMainWindow() | Out-Null
    }
}

$exited = $p.WaitForExit(25000)
if (-not $exited) {
    $p.Kill()
    Write-Host "RESULT:KILLED"
} else {
    Write-Host "RESULT:EXIT_CODE:$($p.ExitCode)"
}
