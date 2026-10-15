$ErrorActionPreference = 'Stop'
$checksums = Get-Content -Raw -LiteralPath release/SHA256SUMS.txt
@(
    '## Ready to publish'
    ''
    "Tag: ``$env:RELEASE_TAG``"
    "Commit: ``$env:RELEASE_SHA``"
    ''
    '```text'
    $checksums.Trim()
    '```'
) -join "`n" | Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY
