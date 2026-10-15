$ErrorActionPreference = 'Stop'
$toolchain = Get-Content -Raw -LiteralPath rust-toolchain.toml
if ($toolchain -notmatch '(?m)^channel\s*=\s*"(\d+\.\d+\.\d+)"') { throw 'Expected an exact Rust version in rust-toolchain.toml.' }
$channel = $Matches[1]
if ($toolchain -notmatch '(?m)^profile\s*=\s*"([a-z]+)"') { throw 'Expected a Rust installation profile.' }
$install = @('toolchain', 'install', $channel, '--profile', $Matches[1], '--no-self-update')
foreach ($entry in @(@('components', '--component'), @('targets', '--target'))) {
    if ($toolchain -match "(?m)^$($entry[0])\s*=\s*\[([^\]]*)\]") {
        foreach ($value in [regex]::Matches($Matches[1], '"([^"\r\n]+)"')) {
            $install += @($entry[1], $value.Groups[1].Value)
        }
    }
}
rustup @install
if ($LASTEXITCODE -ne 0) { throw 'Rust toolchain installation failed.' }
rustup show active-toolchain
if ($LASTEXITCODE -ne 0) { throw 'Rust toolchain activation failed.' }
