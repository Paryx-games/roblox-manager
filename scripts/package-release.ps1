$ErrorActionPreference = 'Stop'
$version =
  $env:RELEASE_TAG -replace '^v', ''

$base =
  "roblox-manager-v$version-windows-x64"

if (-not (Test-Path release/rm_tauri.exe)) {
  throw (
    "rm_tauri.exe was not found in the " +
    "release directory."
  )
}

if (-not (Test-Path release/installer.exe)) {
  throw (
    "installer.exe was not found in the " +
    "release directory."
  )
}

Move-Item `
  -LiteralPath release/rm_tauri.exe `
  -Destination "release/$base.exe"

Move-Item `
  -LiteralPath release/installer.exe `
  -Destination "release/$base-setup.exe"

Set-Content -LiteralPath release/COMMIT.txt -Value $env:RELEASE_SHA -Encoding ascii

$hashes = [ordered]@{}

foreach (
  $file in Get-ChildItem `
    -Path release `
    -File |
    Sort-Object Name
) {
  $hashes[$file.Name] = (
    Get-FileHash `
      -LiteralPath $file.FullName `
      -Algorithm SHA256
  ).Hash.ToLower()
}

$checksums = foreach ($name in $hashes.Keys) {
  "$($hashes[$name])  $name"
}

Set-Content `
  -Path release/SHA256SUMS.txt `
  -Value $checksums
