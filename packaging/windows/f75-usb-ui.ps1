# MIT OR Apache-2.0. Requires Windows PowerShell and WinForms (included in Windows).
[CmdletBinding()]
param([switch]$SelfTest)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$controllerHash = '2cbb346c2e1491ef33755c12186a0e7869027a651eb9bb2c59ba475f77a84129'

function New-ControllerArguments {
    param(
        [ValidateSet('Hold', 'Off', 'Check', 'Diagnose')][string]$Action,
        [string]$Color,
        [int]$Brightness,
        [int]$Seconds,
        [bool]$Continuous
    )
    switch ($Action) {
        'Off' { return 'apply off' }
        'Check' { return 'check' }
        'Diagnose' { return 'diagnose' }
    }
    if ($Color -notmatch '^[0-9a-fA-F]{6}\z') { throw 'Use seis caracteres hexadecimais, por exemplo 00FFFF.' }
    if ($Brightness -lt 0 -or $Brightness -gt 100) { throw 'O brilho deve ficar entre 0 e 100.' }
    if ($Seconds -lt 1 -or $Seconds -gt 3600) { throw 'A duração deve ficar entre 1 e 3600 segundos.' }
    $argsString = 'hold {0} {1}' -f $Color.ToUpperInvariant(), $Brightness
    if (-not $Continuous) { $argsString += ' {0}' -f $Seconds }
    return $argsString
}

function Assert-Controller {
    param([string]$Path, [string]$Expected = $controllerHash)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw 'Extraia o ZIP inteiro: não encontrei o controlador do teclado.' }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Expected) {
        throw 'O controlador não confere com este pacote. Use os arquivos do mesmo ZIP.'
    }
}

function New-ControllerProcessInfo {
    param([string]$Path, [string]$ControllerArgs)
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $Path
    $info.Arguments = $ControllerArgs
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $info.StandardErrorEncoding = [System.Text.Encoding]::UTF8
    return $info
}

function ConvertTo-RgbProfile {
    param($Profile)
    if ($null -eq $Profile) { throw 'Perfil vazio.' }
    foreach ($property in @('Name','Color','Brightness','Seconds','Continuous')) {
        if ($null -eq $Profile.PSObject.Properties[$property]) { throw 'Perfil incompleto.' }
    }
    $name = [string]$Profile.Name
    if ($name -notmatch '^[\p{L}\p{N}][\p{L}\p{N} _-]{0,29}\z' -or $name -ne $name.Trim()) {
        throw 'Use 1 a 30 letras, números, espaços, hífen ou sublinhado no nome.'
    }
    if ($Profile.Continuous -isnot [bool] -or [string]$Profile.Brightness -notmatch '^\d{1,3}\z' -or [string]$Profile.Seconds -notmatch '^\d{1,4}\z') {
        throw 'Brilho, duração ou modo de um perfil são inválidos.'
    }
    [void](New-ControllerArguments Hold ([string]$Profile.Color) ([int]$Profile.Brightness) ([int]$Profile.Seconds) $Profile.Continuous)
    return [pscustomobject][ordered]@{
        Name=$name; Color=([string]$Profile.Color).ToUpperInvariant(); Brightness=[int]$Profile.Brightness;
        Seconds=[int]$Profile.Seconds; Continuous=$Profile.Continuous
    }
}

function Read-RgbProfiles {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return }
    $item = Get-Item -LiteralPath $Path
    if ($item.Length -gt 16384) { throw 'Arquivo de perfis maior que o limite.' }
    $json = [IO.File]::ReadAllText($item.FullName, [Text.Encoding]::UTF8)
    if (-not $json.TrimStart().StartsWith('[')) { throw 'Formato de perfis inválido.' }
    $profiles = @(ConvertFrom-Json -InputObject $json)
    if ($profiles.Count -eq 1 -and $profiles[0] -is [Array]) { $profiles = @($profiles[0]) }
    if ($profiles.Count -gt 20) { throw 'O pacote suporta até 20 perfis.' }
    $seen = @{}
    foreach ($itemProfile in $profiles) {
        $profile = ConvertTo-RgbProfile $itemProfile
        if ($seen.ContainsKey($profile.Name)) { throw 'Arquivo de perfis contém nomes repetidos.' }
        $seen[$profile.Name] = $true
        $profile
    }
}

function Save-RgbProfile {
    param([string]$Path, $Profile)
    $profile = ConvertTo-RgbProfile $Profile
    # Re-read disk on each explicit save so we never overwrite a malformed file
    # or silently forget any other profile.
    $profiles = @(Read-RgbProfiles $Path | Where-Object { $_.Name -ne $profile.Name }) + @($profile)
    if ($profiles.Count -gt 20) { throw 'O pacote suporta até 20 perfis.' }
    $json = ConvertTo-Json -InputObject $profiles -Compress
    $dir = [IO.Path]::GetDirectoryName($Path)
    [void][IO.Directory]::CreateDirectory($dir)
    $temporary = Join-Path $dir ('.profiles-' + [Guid]::NewGuid().ToString('N') + '.tmp')
    try {
        [IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
        # Supplying a unique backup works on .NET Framework as well as .NET.
        if ([IO.File]::Exists($Path)) {
            $backup = $temporary + '.bak'
            [IO.File]::Replace($temporary, $Path, $backup)
            try { [IO.File]::Delete($backup) } catch { Write-Verbose 'Backup kept after explicit profile save.' }
        } else {
            [IO.File]::Move($temporary, $Path)
        }
    } finally {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
    }
}

function Scale-ColorChannel {
    param([int]$Channel, [int]$Level)
    return [int][Math]::Floor(($Channel * $Level + 50) / 100)
}

if ($SelfTest) {
    if ((New-ControllerArguments Hold '00ffff' 35 10 $false) -cne 'hold 00FFFF 35 10') { throw 'Wrong finite command' }
    if ((New-ControllerArguments Hold '00FFFF' 100 10 $true) -cne 'hold 00FFFF 100') { throw 'Wrong continuous command' }
    if ((New-ControllerArguments Off 'bad' 0 0 $false) -cne 'apply off') { throw 'Wrong off command' }
    if ((New-ControllerArguments Diagnose '' 0 0 $false) -cne 'diagnose') { throw 'Wrong diagnostic command' }
    foreach ($case in @(
        @{ Color='00FFFF & dir'; Brightness=35; Seconds=10 },
        @{ Color="00FFFF`n"; Brightness=35; Seconds=10 },
        @{ Color='00FFFF'; Brightness=101; Seconds=10 },
        @{ Color='00FFFF'; Brightness=35; Seconds=0 }
    )) {
        $rejected = $false
        try { [void](New-ControllerArguments -Action Hold @case -Continuous $false) } catch { $rejected = $true }
        if (-not $rejected) { throw 'Invalid controller inputs accepted' }
    }
    $pathWithSpaces = 'C:\a b\f75-usb-c-hidapi.exe'
    $info = New-ControllerProcessInfo $pathWithSpaces 'hold 00FFFF 35 10'
    if ($info.FileName -cne $pathWithSpaces -or $info.Arguments -cne 'hold 00FFFF 35 10' -or $info.UseShellExecute -or -not $info.CreateNoWindow) {
        throw 'Unsafe process configuration or incorrect quoting'
    }
    $rejected = $false
    try { Assert-Controller $PSCommandPath } catch { $rejected = $true }
    if (-not $rejected) { throw 'Unexpected hash accepted' }
    if ((Scale-ColorChannel 255 35) -ne 89 -or (Scale-ColorChannel 255 0) -ne 0) { throw 'Incorrect preview brightness' }
    $tempDir = Join-Path ([IO.Path]::GetTempPath()) ('keylux-selftest-' + [Guid]::NewGuid())
    $profilesTest = Join-Path $tempDir 'profiles.json'
    try {
        if (@(Read-RgbProfiles $profilesTest).Count -ne 0 -or [IO.Directory]::Exists($tempDir)) { throw 'Reading profiles wrote files' }
        $first = [pscustomobject]@{ Name='Código'; Color='00ffff'; Brightness=35; Seconds=10; Continuous=$false }
        $second = [pscustomobject]@{ Name='Noite'; Color='FFB46B'; Brightness=22; Seconds=12; Continuous=$true }
        Save-RgbProfile $profilesTest $first
        Save-RgbProfile $profilesTest $second
        $first.Brightness = 55
        Save-RgbProfile $profilesTest $first
        $read = @(Read-RgbProfiles $profilesTest)
        $updated = $read | Where-Object Name -eq 'Código'
        if ($read.Count -ne 2 -or $updated.Brightness -ne 55 -or $updated.Color -cne '00FFFF') { throw 'Profile round trip failed' }
        [IO.File]::WriteAllText($profilesTest, '{}')
        $badProfileRejected = $false
        try { Save-RgbProfile $profilesTest $first } catch { $badProfileRejected = $true }
        if (-not $badProfileRejected -or [IO.File]::ReadAllText($profilesTest) -cne '{}') { throw 'Malformed profile was overwritten' }
    } finally {
        if ([IO.Directory]::Exists($tempDir)) { [IO.Directory]::Delete($tempDir, $true) }
    }
    Write-Output 'PASS: commands, no shell, hash guard, brightness preview, opt-in profile round trip and malformed-file guard.' 
    return
}

if ($env:OS -ne 'Windows_NT') { throw 'Abra a interface no Windows.' }
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$exe = Join-Path $PSScriptRoot 'f75-usb-c-hidapi.exe'
$script:hex = '00FFFF'
$script:job = $null
$profilePath = Join-Path (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'KeyluxF75') 'profiles.json'
$form = New-Object System.Windows.Forms.Form
$form.Text = 'Keylux | AULA F75'
$form.StartPosition = 'CenterScreen'
$form.ClientSize = [System.Drawing.Size]::new(560, 648)
$form.FormBorderStyle = 'FixedDialog'
$form.MaximizeBox = $false
$form.AutoScroll = $true
$form.AutoScaleMode = 'Font'
$form.Font = [System.Drawing.Font]::new('Segoe UI', 10)
$form.BackColor = [System.Drawing.Color]::FromArgb(245, 248, 251)
$tip = New-Object System.Windows.Forms.ToolTip

function Add-Control {
    param([string]$Type, [string]$Text, [int]$X, [int]$Y, [int]$W, [int]$H)
    $ctrl = New-Object "System.Windows.Forms.$Type"
    $ctrl.Text = $Text
    $ctrl.SetBounds($X, $Y, $W, $H)
    $form.Controls.Add($ctrl)
    return $ctrl
}

$title = Add-Control Label 'Luz do AULA F75' 24 20 510 39
$title.Font = [System.Drawing.Font]::new('Segoe UI Semibold', 22)
$usbLabel = Add-Control Label 'USB-C em modo com fio. Clique em Verificar USB.' 25 66 515 25
[void](Add-Control Label 'Perfil local' 25 104 140 23)
$profileName = Add-Control ComboBox '' 25 130 300 28
$profileName.AccessibleName = 'Nome do perfil'
$profileName.DropDownStyle = 'DropDown'
$profileName.MaxLength = 30
$openProfile = Add-Control Button 'Carregar' 335 128 90 32
$saveProfile = Add-Control Button 'Salvar' 434 128 101 32
$tip.SetToolTip($profileName, 'Escolha um perfil ou digite um nome novo.')
$tip.SetToolTip($saveProfile, 'Salva só neste Windows. Para usar a cor, clique em Aplicar.')
[void](Add-Control Label 'Cor' 25 183 90 24)
$color = Add-Control Button '' 25 208 154 33
$color.AccessibleName = 'Escolher cor'
$color.FlatStyle = 'Flat'
$color.UseVisualStyleBackColor = $false
$tip.SetToolTip($color, 'Abre a paleta de cores')
$presets = @{
    Cyan = Add-Control Button 'Ciano' 189 208 76 33
    Warm = Add-Control Button 'Quente' 271 208 76 33
    Violet = Add-Control Button 'Roxo' 353 208 76 33
    White = Add-Control Button 'Branco' 435 208 76 33
}
function Set-SelectedColor {
    param([string]$Hex)
    $script:hex = $Hex.ToUpperInvariant()
    $rgb = [System.Drawing.ColorTranslator]::FromHtml('#' + $script:hex)
    $color.BackColor = $rgb
    $luma = 0.2126 * $rgb.R + 0.7152 * $rgb.G + 0.0722 * $rgb.B
    $color.ForeColor = if ($luma -ge 135) { [System.Drawing.Color]::Black } else { [System.Drawing.Color]::White }
    $color.Text = '#' + $script:hex + '   Alterar'
    Update-ColorPreview
}
$presets.Cyan.Add_Click({ Set-SelectedColor '00FFFF' })
$presets.Warm.Add_Click({ Set-SelectedColor 'FFB46B' })
$presets.Violet.Add_Click({ Set-SelectedColor '8B5CF6' })
$presets.White.Add_Click({ Set-SelectedColor 'FFFFFF' })
$color.Add_Click({
    $dialog = New-Object System.Windows.Forms.ColorDialog
    try {
        $dialog.Color = $color.BackColor
        $dialog.FullOpen = $true
        if ($dialog.ShowDialog($form) -eq 'OK') {
            Set-SelectedColor ('{0:X2}{1:X2}{2:X2}' -f $dialog.Color.R, $dialog.Color.G, $dialog.Color.B)
        }
    } finally { $dialog.Dispose() }
})

$brightnessLabel = Add-Control Label 'Brilho: 35%' 25 365 240 24
$brightness = Add-Control TrackBar '' 20 389 506 42
$brightness.AccessibleName = 'Brilho de 0 a 100 por cento'
$brightness.Maximum = 100
$brightness.TickFrequency = 10
$brightness.Value = 35
[void](Add-Control Label 'Prévia no brilho escolhido' 25 251 492 22)
$preview = Add-Control Panel '' 25 278 510 78
$preview.AccessibleName = 'Prévia aproximada de toda a iluminação do teclado'
$preview.BackColor = [System.Drawing.Color]::FromArgb(26, 31, 40)
$script:previewColor = [System.Drawing.Color]::Teal
# Approximate 75% layout; artwork not copied from the vendor.
$keyRows = @(
    @(1, -0.45, 1,1,1,1, -0.35, 1,1,1,1, -0.35, 1,1,1,1, -0.30,1.15),
    @(1,1,1,1,1,1,1,1,1,1,1,1,1,2,1),
    @(1.5,1,1,1,1,1,1,1,1,1,1,1,1,1.5,1),
    @(1.75,1,1,1,1,1,1,1,1,1,1,1,2.25,1),
    @(2.25,1,1,1,1,1,1,1,1,1,1,1.75,1,1),
    @(1.25,1.25,1.25,6.25,1,1,1,1,1,1)
)
$preview.Add_Paint({
    param($sender, $eventArgs)
    $brush = [System.Drawing.SolidBrush]::new($script:previewColor)
    $rim = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(104, 115, 131))
    try {
        $keyUnit = ($sender.ClientSize.Width - 22) / 16.6
        $rowPitch = ($sender.ClientSize.Height - 15) / $keyRows.Count
        $y = 7.0
        foreach ($row in $keyRows) {
            $x = 10.0
            foreach ($width in $row) {
                $step = [Math]::Abs([double]$width) * $keyUnit
                if ($width -gt 0) {
                    $w = [single]($step - 3)
                    $h = [single]($rowPitch - 3)
                    $eventArgs.Graphics.FillRectangle($brush, [single]$x, [single]$y, $w, $h)
                    $eventArgs.Graphics.DrawRectangle($rim, [single]$x, [single]$y, $w, $h)
                }
                $x += $step
            }
            $y += $rowPitch
        }
    } finally {
        $brush.Dispose()
        $rim.Dispose()
    }
})
function Update-ColorPreview {
    $rgb = [System.Drawing.ColorTranslator]::FromHtml('#' + $script:hex)
    $script:previewColor = [System.Drawing.Color]::FromArgb(
        (Scale-ColorChannel $rgb.R $brightness.Value),
        (Scale-ColorChannel $rgb.G $brightness.Value),
        (Scale-ColorChannel $rgb.B $brightness.Value))
    $preview.Invalidate()
}
$brightness.Add_ValueChanged({
    $brightnessLabel.Text = 'Brilho: {0}%' -f $brightness.Value
    Update-ColorPreview
})
Set-SelectedColor $script:hex

[void](Add-Control Label 'Segundos' 25 451 86 23)
$seconds = Add-Control NumericUpDown '' 112 449 85 28
$seconds.AccessibleName = 'Duração em segundos'
$seconds.Minimum = 1
$seconds.Maximum = 3600
$seconds.Value = 10
$continuous = Add-Control CheckBox 'Até eu parar' 224 448 166 28
$continuous.Add_CheckedChanged({ $seconds.Enabled = -not $continuous.Checked })

$start = Add-Control Button 'Aplicar' 25 492 116 37
$stop = Add-Control Button 'Parar' 149 492 91 37
$off = Add-Control Button 'Apagar' 248 492 91 37
$check = Add-Control Button 'Verificar USB' 347 492 105 37
$diagnose = Add-Control Button 'Detalhes' 460 492 75 37
$stop.Enabled = $false
$tip.SetToolTip($stop, 'Para o envio. Para apagar o último quadro, use Apagar.')
$tip.SetToolTip($diagnose, 'Mostra as etapas da identificação USB, sem enviar uma cor.')
$status = Add-Control TextBox '' 25 542 510 80
$status.AccessibleName = 'Resultado do controlador'
$status.Multiline = $true
$status.ReadOnly = $true
$status.ScrollBars = 'Vertical'
$status.Text = 'Escolha a cor e clique em Aplicar. Parar interrompe o envio, sem apagar a última cor.'

function Refresh-ProfileList {
    param([string]$Selection)
    $profiles = @(Read-RgbProfiles $profilePath)
    $profileName.Items.Clear()
    foreach ($profile in $profiles) { [void]$profileName.Items.Add($profile.Name) }
    $profileName.Text = $Selection
}
try { Refresh-ProfileList '' } catch { $status.Text = 'Não foi possível ler os perfis. O arquivo foi mantido.' }
$saveProfile.Add_Click({
    try {
        $name = $profileName.Text.Trim()
        $profile = [pscustomobject]@{
            Name=$name; Color=$script:hex; Brightness=$brightness.Value;
            Seconds=[int]$seconds.Value; Continuous=$continuous.Checked
        }
        if (@(Read-RgbProfiles $profilePath | Where-Object Name -eq $name).Count -gt 0) {
            $answer = [System.Windows.Forms.MessageBox]::Show($form, "Substituir o perfil $name?", 'Salvar perfil', 'YesNo', 'Question')
            if ($answer -ne 'Yes') { return }
        }
        Save-RgbProfile $profilePath $profile
        Refresh-ProfileList $name
        $status.Text = 'Perfil salvo só neste Windows.'
    } catch { $status.Text = $_.Exception.Message }
})
$openProfile.Add_Click({
    try {
        $name = $profileName.Text.Trim()
        $profile = Read-RgbProfiles $profilePath | Where-Object Name -eq $name
        if ($null -eq $profile) { throw 'Escolha um perfil salvo para carregar.' }
        Set-SelectedColor $profile.Color
        $brightness.Value = $profile.Brightness
        $seconds.Value = $profile.Seconds
        $continuous.Checked = $profile.Continuous
        $status.Text = 'Perfil carregado. Clique em Aplicar para enviar a cor.'
    } catch { $status.Text = $_.Exception.Message }
})

function Set-Running {
    param([bool]$Running)
    foreach ($ctrl in @($start, $off, $check, $diagnose, $color, $brightness, $continuous, $profileName, $openProfile, $saveProfile) + @($presets.Values)) {
        $ctrl.Enabled = -not $Running
    }
    $seconds.Enabled = (-not $Running) -and (-not $continuous.Checked)
    $stop.Enabled = $Running
}

function Start-ControllerAction {
    param([string]$Action)
    if ($null -ne $script:job) { return }
    $p = $null
    $started = $false
    try {
        $argList = New-ControllerArguments $Action $script:hex $brightness.Value ([int]$seconds.Value) $continuous.Checked
        Assert-Controller $exe
        $p = New-Object System.Diagnostics.Process
        $p.StartInfo = New-ControllerProcessInfo $exe $argList
        $started = $p.Start()
        if (-not $started) { throw 'Não consegui abrir o controlador.' }
        $script:job = @{
            Process=$p; Out=$p.StandardOutput.ReadToEndAsync(); Err=$p.StandardError.ReadToEndAsync();
            Action=$Action; Stopped=$false
        }
        Set-Running $true
        $status.Text = if ($Action -eq 'Hold') { 'Aplicando após verificar o F75. Se precisar, clique em Parar.' } else { 'Aguarde…' }
    } catch {
        if ($null -ne $p -and $null -eq $script:job) {
            try { if ($started -and -not $p.HasExited) { $p.Kill() } } finally { $p.Dispose() }
        }
        $status.Text = $_.Exception.Message
    }
}

function Stop-ControllerAction {
    if ($null -eq $script:job) { return }
    $p = $script:job.Process
    if ($p.HasExited) { return }
    try {
        $p.Kill()
        $script:job.Stopped = $true
        $status.Text = 'Parando…'
    } catch {
        if (-not $p.HasExited) { $status.Text = 'Não consegui interromper o controlador. Tente novamente.' }
    }
}

$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 150
$timer.Add_Tick({
    if ($null -eq $script:job -or -not $script:job.Process.HasExited) { return }
    $job = $script:job
    if (-not $job.Out.IsCompleted -or -not $job.Err.IsCompleted) { return }
    try {
        $outText = $job.Out.GetAwaiter().GetResult().Trim()
        $errorText = $job.Err.GetAwaiter().GetResult().Trim()
        if ($job.Action -eq 'Check' -and -not $job.Stopped) {
            $usbLabel.Text = if ($job.Process.ExitCode -eq 0) { 'Última verificação: F75 confirmado pelo USB-C.' } else { 'Última verificação: F75 não confirmado.' }
        }
        $status.Text = if ($job.Stopped) {
            'Envio interrompido. Use Apagar se quiser apagar a última cor.'
        } elseif ($job.Process.ExitCode -ne 0) {
            'O comando falhou. Confira o cabo e feche outros apps de RGB.' + [Environment]::NewLine + $errorText
        } elseif ($job.Action -eq 'Hold') {
            'Envio concluído. Confira a cor no teclado; para apagar, clique em Apagar.'
        } elseif ($job.Action -eq 'Off') {
            'Comando para apagar enviado. Confira o teclado.'
        } else {
            $outText
        }
    } catch {
        $status.Text = $_.Exception.Message
    } finally {
        $job.Process.Dispose()
        $script:job = $null
        Set-Running $false
    }
})
$start.Add_Click({ Start-ControllerAction Hold })
$stop.Add_Click({ Stop-ControllerAction })
$off.Add_Click({ Start-ControllerAction Off })
$check.Add_Click({ Start-ControllerAction Check })
$diagnose.Add_Click({ Start-ControllerAction Diagnose })
$form.Add_FormClosing({
    param($sender, $eventArgs)
    if ($null -eq $script:job) { return }
    Stop-ControllerAction
    if (-not $script:job.Process.WaitForExit(2000)) {
        $eventArgs.Cancel = $true
        $status.Text = 'O envio ainda está parando. Aguarde e tente fechar novamente.'
    }
})
try {
    $timer.Start()
    [void]$form.ShowDialog()
} finally {
    $timer.Stop()
    $timer.Dispose()
    if ($null -ne $script:job) { $script:job.Process.Dispose() }
    $tip.Dispose()
    $form.Dispose()
}
