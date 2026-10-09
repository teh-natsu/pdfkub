<#
.SYNOPSIS
  Install (or update) PdfKub for the current user from a release build, without an installer.
.DESCRIPTION
  Copies pdfkub.exe and pdfkub-cli.exe to %LOCALAPPDATA%\Programs\PdfKub, adds Start menu and
  desktop shortcuts, and offers PdfKub under "Open with" for PDF files (it does not become the
  default PDF app). No administrator rights are needed. -Uninstall removes all of it; settings in
  %APPDATA%\PdfKub are kept.
.EXAMPLE
  cargo build --release -p pdfkub -p pdfkub-cli
  powershell -ExecutionPolicy Bypass -File packaging/windows/install-user.ps1
  powershell -ExecutionPolicy Bypass -File packaging/windows/install-user.ps1 -Uninstall
#>
param([switch] $Uninstall)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Dest = Join-Path $env:LOCALAPPDATA 'Programs\PdfKub'
$StartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\PdfKub.lnk'
$Desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'PdfKub.lnk'
$AppKey = 'HKCU:\Software\Classes\Applications\pdfkub.exe'
$ProgId = 'HKCU:\Software\Classes\PdfKub.Document'
$OpenWith = 'HKCU:\Software\Classes\.pdf\OpenWithProgids'

if ($Uninstall) {
  Get-Process pdfkub -ErrorAction SilentlyContinue | Stop-Process
  foreach ($p in @($StartMenu, $Desktop, $Dest, $AppKey, $ProgId)) {
    if (Test-Path $p) { Remove-Item $p -Recurse -Force }
  }
  if (Test-Path $OpenWith) { Remove-ItemProperty -Path $OpenWith -Name 'PdfKub.Document' -ErrorAction SilentlyContinue }
  Write-Output 'PdfKub removed (settings in %APPDATA%\PdfKub kept).'
  return
}

$Release = Join-Path $Root 'target\release'
foreach ($exe in 'pdfkub.exe', 'pdfkub-cli.exe') {
  if (-not (Test-Path (Join-Path $Release $exe))) { throw "$exe not found: run cargo build --release -p pdfkub -p pdfkub-cli first" }
}
# A running copy locks its executable.
Get-Process pdfkub -ErrorAction SilentlyContinue | Stop-Process
Start-Sleep -Milliseconds 500
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
foreach ($exe in 'pdfkub.exe', 'pdfkub-cli.exe') { Copy-Item (Join-Path $Release $exe) $Dest -Force }
$App = Join-Path $Dest 'pdfkub.exe'

$shell = New-Object -ComObject WScript.Shell
foreach ($lnk in @($StartMenu, $Desktop)) {
  $s = $shell.CreateShortcut($lnk)
  $s.TargetPath = $App
  $s.WorkingDirectory = $Dest
  $s.IconLocation = "$App,0"
  $s.Description = 'PdfKub PDF workbench'
  $s.Save()
}

# "Open with" for PDFs (per user; the default PDF app is left alone).
New-Item -Path "$ProgId\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path $ProgId -Name '(default)' -Value 'PDF Document (PdfKub)'
New-Item -Path "$ProgId\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "$ProgId\DefaultIcon" -Name '(default)' -Value "`"$App`",0"
Set-ItemProperty -Path "$ProgId\shell\open\command" -Name '(default)' -Value "`"$App`" `"%1`""
New-Item -Path "$AppKey\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path $AppKey -Name 'FriendlyAppName' -Value 'PdfKub'
Set-ItemProperty -Path "$AppKey\shell\open\command" -Name '(default)' -Value "`"$App`" `"%1`""
New-Item -Path "$AppKey\SupportedTypes" -Force | Out-Null
Set-ItemProperty -Path "$AppKey\SupportedTypes" -Name '.pdf' -Value ''
New-Item -Path $OpenWith -Force | Out-Null
Set-ItemProperty -Path $OpenWith -Name 'PdfKub.Document' -Value ''

Write-Output "PdfKub installed in $Dest (Start menu and desktop shortcuts, 'Open with' for PDFs)."
