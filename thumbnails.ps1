<#
.SYNOPSIS
  Enable or disable the portable PDF thumbnail provider for the current user.
  Keep this script and pdfcraft_thumbnail.dll together in a permanent folder.
  Uses HKCU only, preserves the previous per-user provider, and never changes the PDF default app.
#>
param(
  [ValidateSet('Enable', 'Disable')] [string] $Action = 'Enable',
  [switch] $ReplaceExisting
)
$ErrorActionPreference = 'Stop'
$Provider = '{D4B81246-583F-4868-9DE4-D54B51F160A9}'
$Category = '{E357FCCD-A995-4576-B01F-234630154E96}'
$Binding = "Software\Classes\.pdf\shellex\$Category"
$Class = "Software\Classes\CLSID\$Provider"
$Backup = 'Software\PdfCraft\ThumbnailBackup'
$UserRegistry = [Microsoft.Win32.Registry]::CurrentUser

function Read-Default($Hive, [string] $Key) {
  $handle = $Hive.OpenSubKey($Key)
  if (-not $handle) { return $null }
  try { return $handle.GetValue('', $null) } finally { $handle.Dispose() }
}
function Write-String([string] $Key, [string] $Name, [string] $Value) {
  $handle = $UserRegistry.CreateSubKey($Key)
  try { $handle.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String) }
  finally { $handle.Dispose() }
}

if ($Action -eq 'Enable') {
  $Dll = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'pdfcraft_thumbnail.dll')).Path
  # Check machine type before registering a DLL that Explorer cannot load.
  $bytes = [IO.File]::ReadAllBytes($Dll)
  if ($bytes.Length -lt 64) { throw 'Invalid thumbnail DLL' }
  $pe = [BitConverter]::ToInt32($bytes, 0x3c)
  if ($pe -lt 0 -or $pe -gt ($bytes.Length - 24) -or [BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550) { throw 'Invalid PE header' }
  $machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
  $arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
  $expected = switch ($arch) { 'X64' { 0x8664 } 'X86' { 0x14c } 'Arm64' { 0xaa64 } default { throw "Unsupported OS architecture: $arch" } }
  if ($machine -ne $expected) { throw "Use the thumbnail DLL for $arch Windows" }
  $Previous = Read-Default $UserRegistry $Binding
  $Effective = if ($null -ne $Previous) { $Previous } else { Read-Default ([Microsoft.Win32.Registry]::LocalMachine) $Binding }
  if ($Effective -and $Effective -ne $Provider -and -not $ReplaceExisting) {
    throw "Another PDF thumbnail provider is registered ($Effective). To replace it deliberately, run with -ReplaceExisting."
  }
  if ($Previous -ne $Provider) {
    Write-String $Backup 'HadPrevious' ([string]($null -ne $Previous))
    Write-String $Backup 'Previous' ([string]$Previous)
  }
  Write-String $Class '' 'PdfCraft PDF Thumbnail Provider'
  Write-String "$Class\InprocServer32" '' $Dll
  Write-String "$Class\InprocServer32" 'ThreadingModel' 'Both'
  Write-String $Binding '' $Provider
  Write-Output 'PDF thumbnails enabled for your Windows account. Keep this folder in place. Sign out and back in to reload the thumbnail handler.'
} else {
  if ((Read-Default $UserRegistry $Binding) -eq $Provider) {
    $handle = $UserRegistry.OpenSubKey($Backup)
    try {
      if ($handle -and $handle.GetValue('HadPrevious') -eq 'True') {
        Write-String $Binding '' ([string]$handle.GetValue('Previous', ''))
      } else {
        $bindingHandle = $UserRegistry.OpenSubKey($Binding, $true)
        try { if ($bindingHandle) { $bindingHandle.DeleteValue('', $false) } }
        finally { if ($bindingHandle) { $bindingHandle.Dispose() } }
      }
    } finally { if ($handle) { $handle.Dispose() } }
  }
  # Exact, project-owned registry keys only. Never delete a file or someone else's provider.
  $UserRegistry.DeleteSubKeyTree($Class, $false)
  $UserRegistry.DeleteSubKeyTree($Backup, $false)
  Write-Output 'PdfCraft thumbnail registration removed. Sign out and back in to reload the previous provider.'
}
