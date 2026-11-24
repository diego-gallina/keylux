# MIT OR Apache-2.0. Optional local-only helper; makes no firmware/config writes.
[CmdletBinding()]
param(
    [ValidateSet('Watch', 'Install', 'Remove', 'SelfTest')]
    [string]$Action,
    [ValidatePattern('^[0-9A-Fa-f]{6}\z')]
    [string]$Color,
    [ValidateRange(0, 100)]
    [int]$Brightness = 35,
    [string]$ExePath = (Join-Path $PSScriptRoot 'f75-usb-c-hidapi.exe'),
    [ValidatePattern('^[0-9A-Fa-f]{64}\z')]
    [string]$ExeSha256 = '598c4a9873878b74bf13c324487b74b0ad05af29bbdf371bd6356c8f462a0280'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Invoke-KeyluxTick {
    param(
        [hashtable]$State,
        [bool]$Present,
        [scriptblock]$ConfirmKeyboard,
        [scriptblock]$Reapply
    )
    if (-not $Present) {
        $State.Attempts = 0
        $State.Finished = $false
        return
    }
    if ($State.Finished -or $State.Attempts -ge 3) { return }
    $State.Attempts += 1
    if (& $ConfirmKeyboard) {
        # Even if apply errors, do not repeat a possibly delivered HID write.
        $State.Finished = $true
        & $Reapply
    }
}

function Assert-Test {
    param([bool]$Passed, [string]$Explanation)
    if (-not $Passed) { throw $Explanation }
}

function Invoke-SelfTest {
    $state = @{ Attempts = 0; Finished = $false }
    $observed = @{ Checks = 0; Writes = 0; Verified = $false; FailWrite = $false }
    $check = { $observed.Checks += 1; $observed.Verified }
    $apply = {
        $observed.Writes += 1
        if ($observed.FailWrite) { throw 'simulated write failure' }
    }
    Invoke-KeyluxTick $state $false $check $apply
    Assert-Test ($observed.Checks -eq 0) 'Absent hardware must not be checked.'
    1..7 | ForEach-Object { Invoke-KeyluxTick $state $true $check $apply }
    Assert-Test ($observed.Checks -eq 3 -and $observed.Writes -eq 0) 'Failed confirmation must never send RGB; retry budget is three.'
    $observed.Verified = $true
    Invoke-KeyluxTick $state $true $check $apply
    Assert-Test ($observed.Writes -eq 0) 'No additional retries before a detected reconnect.'
    Invoke-KeyluxTick $state $false $check $apply
    Invoke-KeyluxTick $state $true $check $apply
    Invoke-KeyluxTick $state $true $check $apply
    Assert-Test ($observed.Checks -eq 4 -and $observed.Writes -eq 1) 'New connection applies one frame after confirmation.'
    Invoke-KeyluxTick $state $false $check $apply
    $observed.FailWrite = $true
    try { Invoke-KeyluxTick $state $true $check $apply } catch {
        if ($_.Exception.Message -ne 'simulated write failure') { throw }
    }
    Invoke-KeyluxTick $state $true $check $apply
    Assert-Test ($observed.Checks -eq 5 -and $observed.Writes -eq 2) 'A failed write must not be blindly retried.'
    Write-Output 'PASS: no hardware, no unconfirmed writes, bounded retries, one apply per connection.'
}

function Test-UsbPresent {
    # The USB parent (not any arbitrary vendor collection or Bluetooth device).
    foreach ($device in (Get-PnpDevice -PresentOnly)) {
        if ($device.InstanceId -like 'USB\VID_258A&PID_010C\*') { return $true }
    }
    return $false
}

function Assert-KeyluxHash {
    param([string]$Path)
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $ExeSha256) {
        throw 'Keylux hash does not match. Nothing was sent to the keyboard.'
    }
}

function Invoke-KeyluxExe {
    param([string]$Path, [string[]]$Arguments)
    Assert-KeyluxHash $Path
    $process = Start-Process -FilePath $Path -ArgumentList $Arguments -NoNewWindow -PassThru
    try {
        if (-not $process.WaitForExit(8000)) {
            if (-not $process.HasExited) { $process.Kill() }
            if (-not $process.WaitForExit(2000)) { throw 'Cannot stop timed-out Keylux process.' }
            return -1
        }
        return $process.ExitCode
    } finally {
        $process.Dispose()
    }
}

if (-not $Action) {
    Write-Output 'Specify Watch (manual), Install (login opt-in), Remove, or SelfTest. Watch/Install require -Color RRGGBB.'
    return
}
if ($Action -eq 'SelfTest') {
    Invoke-SelfTest
    return
}

if ($env:OS -ne 'Windows_NT') { throw 'This helper runs on Windows only.' }
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$taskName = 'Keylux-F75-USB-' + $identity.User.Value
if ($Action -eq 'Remove') {
    $scheduled = Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue
    if ($scheduled) {
        if ($scheduled.State -eq 'Running') { Stop-ScheduledTask -InputObject $scheduled }
        Unregister-ScheduledTask -TaskName $taskName -TaskPath '\' -Confirm:$false
    }
    Write-Output 'Keylux login task removed (or already absent). Stop any manual watcher with Ctrl+C.'
    return
}
if (-not $Color) { throw 'Pick an explicit color with -Color RRGGBB (example: 00FFFF).' }
$exe = Get-Item -LiteralPath $ExePath
if ($exe.PSIsContainer -or $exe.Extension -ne '.exe') { throw 'ExePath must point to the reviewed Keylux executable.' }
# Restrict to a local Windows disk path, never a remote share.
if ($exe.FullName -notmatch '^[A-Za-z]:\\[^"]+$' -or $PSCommandPath -notmatch '^[A-Za-z]:\\[^"]+$') {
    throw 'Use ordinary local Windows paths for both script and executable.'
}
Assert-KeyluxHash $exe.FullName

if ($Action -eq 'Install') {
    $powerShell = Join-Path ([Environment]::GetFolderPath('System')) 'WindowsPowerShell\v1.0\powershell.exe'
    $taskArgs = '-NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -File "{0}" -Action Watch -Color {1} -Brightness {2} -ExePath "{3}" -ExeSha256 {4}' -f $PSCommandPath, $Color, $Brightness, $exe.FullName, $ExeSha256
    $taskAction = New-ScheduledTaskAction -Execute $powerShell -Argument $taskArgs
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity.Name
    $principal = New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit ([timespan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    # Do not replace an existing task or run anything until the next login.
    Register-ScheduledTask -TaskName $taskName -TaskPath '\' -Action $taskAction -Trigger $trigger -Principal $principal -Settings $settings -Description 'Opt-in temporary RGB for a single verified AULA F75 USB-C.' | Out-Null
    Write-Output 'Keylux will watch for the keyboard at your next Windows login. Use -Action Remove to undo.'
    return
}

$mutex = [System.Threading.Mutex]::new($false, ('Local\' + $taskName))
$ownsMutex = $false
try {
    try { $ownsMutex = $mutex.WaitOne(0) } catch [System.Threading.AbandonedMutexException] { $ownsMutex = $true }
    if (-not $ownsMutex) { throw 'Keylux watcher already running in this Windows session.' }
    $state = @{ Attempts = 0; Finished = $false }
    $confirm = { (Invoke-KeyluxExe $exe.FullName @('check')) -eq 0 }
    $apply = {
        if ((Invoke-KeyluxExe $exe.FullName @('apply', $Color, "$Brightness")) -ne 0) {
            Write-Warning 'Color not confirmed; will not send it again until a detected reconnect.'
        }
    }
    while ($true) {
        Invoke-KeyluxTick $state (Test-UsbPresent) $confirm $apply
        Start-Sleep -Seconds 5
    }
} finally {
    if ($ownsMutex) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
