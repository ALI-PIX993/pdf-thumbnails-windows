param(
  [switch] $PrepareOnly,
  [string] $SourceRepository = 'https://github.com/storytold/pdfcraft'
)
$ErrorActionPreference = 'Stop'
$Root = $PSScriptRoot
$Checkout = Join-Path $Root 'build/pdfcraft'
$Revision = '5d590604867fdac7ed71c555c40477ae3331a60a'
function Run-Git([string[]] $Arguments) {
  & git @Arguments
  if ($LASTEXITCODE -ne 0) { throw "Git failed: $Arguments" }
}
if (Test-Path -LiteralPath $Checkout) {
  throw 'Build checkout already exists. Use a fresh project copy for a clean rebuild; no existing files are overwritten.'
}
New-Item -ItemType Directory -Force -Path $Checkout | Out-Null
Run-Git @('init', $Checkout)
Run-Git @('-C', $Checkout, 'remote', 'add', 'origin', $SourceRepository)
Run-Git @('-C', $Checkout, 'fetch', '--depth=1', 'origin', $Revision)
Run-Git @('-C', $Checkout, 'checkout', '--detach', 'FETCH_HEAD')
Run-Git @('-C', $Checkout, 'apply', '--check', (Join-Path $Root 'renderer-thumbnail.patch'))
Run-Git @('-C', $Checkout, 'apply', (Join-Path $Root 'renderer-thumbnail.patch'))
$ProviderDir = Join-Path $Checkout 'apps/pdfcraft-thumbnail'
New-Item -ItemType Directory -Force -Path $ProviderDir | Out-Null
Copy-Item "$Root/Cargo.toml", "$Root/lib.rs", "$Root/windows.rs" -Destination $ProviderDir
Copy-Item -LiteralPath (Join-Path $Root 'Cargo.lock') -Destination (Join-Path $Checkout 'Cargo.lock')
if ($PrepareOnly) { Write-Output "Prepared $Checkout"; exit 0 }
if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
  throw 'The installer package currently supports Windows x64 only.'
}
Push-Location $Checkout
try {
  & cargo test --locked -p pdfcraft-render --lib
  if ($LASTEXITCODE -ne 0) { throw 'Renderer tests failed.' }
  # The upstream lockfile predates the provider; allow Cargo to add Windows COM dependencies.
  & cargo test --locked -p pdfcraft-thumbnail --lib
  if ($LASTEXITCODE -ne 0) { throw 'Native provider tests failed.' }
  & cargo build --locked --release -p pdfcraft-thumbnail
  if ($LASTEXITCODE -ne 0) { throw 'Provider build failed.' }
} finally { Pop-Location }
$Dist = Join-Path $Root 'dist/windows-x64'
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Copy-Item -LiteralPath (Join-Path $Checkout 'target/release/pdfcraft_thumbnail.dll') -Destination $Dist
Copy-Item "$PSScriptRoot/thumbnails.ps1", "$PSScriptRoot/*.cmd" -Destination $Dist
Copy-Item "$Root/LICENSE-*", "$Root/NOTICE", "$Root/README.md" -Destination $Dist
Copy-Item "$Checkout/NOTICE" -Destination (Join-Path $Dist 'UPSTREAM-NOTICE')
Copy-Item "$Checkout/ATTRIBUTION.toml", "$Checkout/ATTRIBUTION.md" -Destination $Dist
Expand-Archive -LiteralPath (Join-Path $Root 'third-party-licenses.zip') -DestinationPath $Dist -Force
Write-Output "Built $Dist. Retain dependency license texts before redistributing a binary; see NOTICE."
