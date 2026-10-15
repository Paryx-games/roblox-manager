$ErrorActionPreference = "Stop"

$checksumPath =
  "release/SHA256SUMS.txt"

if (-not (Test-Path -LiteralPath $checksumPath)) {
  throw "SHA256SUMS.txt is missing."
}

$expected = @{}

foreach (
  $line in Get-Content `
    -LiteralPath $checksumPath
) {
  if (
    $line -notmatch
      '^([0-9a-fA-F]{64})\s{2}(.+)$'
  ) {
    throw (
      "Invalid SHA256SUMS.txt line: " +
      "'$line'"
    )
  }

  $expected[$Matches[2]] =
    $Matches[1].ToLower()
}

$files = @(
  Get-ChildItem `
    -Path release `
    -File |
    Where-Object { $_.Name -like '*.exe' -or $_.Name -like '*-portable.zip' } |
    Sort-Object Name
)

if ($files.Count -ne 2) {
  throw (
    "Expected exactly two release downloads, " +
    "found $($files.Count)."
  )
}
foreach ($file in $files) {
  if (-not $expected.ContainsKey($file.Name)) {
    throw (
      "No expected SHA256 exists for " +
      "$($file.Name)."
    )
  }

  $actual = (
    Get-FileHash `
      -LiteralPath $file.FullName `
      -Algorithm SHA256
  ).Hash.ToLower()

  if (
    $actual -ne
    $expected[$file.Name]
  ) {
    throw (
      "SHA256 mismatch for $($file.Name). " +
      "The packaged artifact changed."
    )
  }
}

function New-VtResult {
  param(
    [string]$Name,
    [string]$Hash,
    [string]$Message,
    [string]$ReportUrl = "",
    [int]$Flagged = -1,
    [int]$Total = -1
  )

  return [pscustomobject]@{
    Name = $Name
    Hash = $Hash
    Message = $Message
    ReportUrl = $ReportUrl
    Flagged = $Flagged
    Total = $Total
  }
}

function Scan-VirusTotal {
  param(
    [System.IO.FileInfo]$File,
    [string]$Hash
  )

  if (-not $env:VT_API_KEY) {
    return New-VtResult `
      -Name $File.Name `
      -Hash $Hash `
      -Message (
        "Scan unavailable for this build."
      )
  }

  $headers = @{
    "x-apikey" = $env:VT_API_KEY
  }

  try {
    $known = Invoke-RestMethod -Uri "https://www.virustotal.com/api/v3/files/$Hash" -Headers $headers -TimeoutSec 20
    $stats = $known.data.attributes.last_analysis_stats
    $total = 0
    foreach ($property in $stats.PSObject.Properties) { $total += [int]$property.Value }
    if ($total -gt 0) {
      $flagged = [int]$stats.malicious + [int]$stats.suspicious
      return New-VtResult -Name $File.Name -Hash $Hash -ReportUrl "https://www.virustotal.com/gui/file/$Hash" -Flagged $flagged -Total $total -Message "$flagged / $total security vendors flagged this file"
    }
  } catch {
    Write-Host "::warning::VirusTotal hash lookup failed; trying upload."
  }

  $uploadUri =
    "https://www.virustotal.com/api/v3/files"

  if ($File.Length -gt 32MB) {
    try {
      $uploadUri = (
        Invoke-RestMethod `
          -Uri "$uploadUri/upload_url" `
          -Headers $headers `
          -TimeoutSec 20
      ).data
    } catch {
      return New-VtResult `
        -Name $File.Name `
        -Hash $Hash `
        -Message (
          "Scan unavailable: could not obtain " +
          "a VirusTotal upload URL. " +
          "$($_.Exception.Message)"
        )
    }
  }

  $upload = $null
  $lastUploadError = ""
  foreach ($delay in @(0, 15)) {
    if ($delay -gt 0) {
      Start-Sleep -Seconds $delay
    }

    try {
      if ($script:lastUploadAt) {
        $remaining = 20 - ([DateTime]::UtcNow - $script:lastUploadAt).TotalSeconds
        if ($remaining -gt 0) { Start-Sleep -Milliseconds ([int][Math]::Ceiling($remaining * 1000)) }
      }
      $script:lastUploadAt = [DateTime]::UtcNow
      $upload = Invoke-RestMethod `
        -Uri $uploadUri `
        -Method Post `
        -Headers $headers `
        -Form @{
          file = $File
        } `
        -TimeoutSec 45

      if (
        $upload -and
        $upload.data -and
        $upload.data.id
      ) {
        break
      }
    } catch {
      $lastUploadError =
        $_.Exception.Message

      Write-Host (
        "::warning::VirusTotal upload for " +
        "$($File.Name) failed: " +
        "$lastUploadError"
      )
    }
  }

  if (
    -not $upload -or
    -not $upload.data -or
    -not $upload.data.id
  ) {
    return New-VtResult `
      -Name $File.Name `
      -Hash $Hash `
      -Message (
        "Scan unavailable: upload failed " +
        "after multiple attempts. " +
        "$lastUploadError"
      )
  }

  $analysisId =
    $upload.data.id

  $reportUrl =
    "https://www.virustotal.com/gui/file/$Hash"

  $analysis = $null
  for ($i = 0; $i -lt 6; $i++) {
    Start-Sleep -Seconds 20

    try {
      $candidate =
        Invoke-RestMethod `
          -Uri (
            "https://www.virustotal.com/" +
            "api/v3/analyses/$analysisId"
          ) `
          -Headers $headers `
          -TimeoutSec 15
    } catch {
      Write-Host (
        "::warning::VirusTotal polling for " +
        "$($File.Name) failed: " +
        "$($_.Exception.Message)"
      )

      continue
    }

    if (
      $candidate.data.attributes.status -eq
      "completed"
    ) {
      $analysis = $candidate
      break
    }
  }

  if (-not $analysis) {
    return New-VtResult `
      -Name $File.Name `
      -Hash $Hash `
      -ReportUrl $reportUrl `
      -Message (
        "Scan unavailable: VirusTotal did not " +
        "finish within the polling window."
      )
  }

  $stats =
    $analysis.data.attributes.stats

  $flagged =
    [int]$stats.malicious +
    [int]$stats.suspicious

  $total = 0

  foreach (
    $property in $stats.PSObject.Properties
  ) {
    $total +=
      [int]$property.Value
  }

  return New-VtResult `
    -Name $File.Name `
    -Hash $Hash `
    -ReportUrl $reportUrl `
    -Flagged $flagged `
    -Total $total `
    -Message (
      "$flagged / $total security vendors " +
      "flagged this file"
    )
}

$script:lastUploadAt = $null
$results = @()

for ($i = 0; $i -lt $files.Count; $i++) {
  $file = $files[$i]
  $hash = $expected[$file.Name]
  try {
    $result =
      Scan-VirusTotal `
        -File $file `
        -Hash $hash
  } catch {
    $result =
      New-VtResult `
        -Name $file.Name `
        -Hash $hash `
        -Message (
          "Scan unavailable: " +
          "$($_.Exception.Message)"
        )
  }

  $results += $result

  if (
    $result.Flagged -gt 0 -or
    $result.Flagged -lt 0
  ) {
    Write-Host (
      "::warning::$($result.Name): " +
      "$($result.Message)"
    )
  }
}

$workflowUrl =
  "https://github.com/$env:REPO/blob/" +
  "$env:TAG_SHA/.github/workflows/release.yml"

$security = @(
  "## Security"
  ""
  "GitHub build provenance attestations are generated for release downloads and manifests."
  ""
  "Verify a downloaded artifact with:"
  ""
  '```text'
  "gh attestation verify <file> --repo $env:REPO"
  '```'
  ""
  "### VirusTotal"
  ""
  "VirusTotal scanning is best-effort and does not block a release if the external service is unavailable."
  ""
)

foreach ($result in $results) {
  $security += @(
    "- **``$($result.Name)``**"
    "  - SHA256: ``$($result.Hash)``"
    "  - Scan result: $($result.Message)"
  )

  if ($result.ReportUrl) {
    $security +=
      "  - Report: $($result.ReportUrl)"
  }

  $security += ""
}

$security +=
  "- **Build workflow:** [``release.yml``]($workflowUrl)"

Set-Content `
  -Path security.md `
  -Value ($security -join "`n")

$summary = @(
  "## VirusTotal"
  ""
)

foreach ($result in $results) {
  $summary += @(
    "### $($result.Name)"
    ""
    "- SHA256: ``$($result.Hash)``"
    "- $($result.Message)"
  )

  if ($result.ReportUrl) {
    $summary +=
      "- [Open VirusTotal report]($($result.ReportUrl))"
  }

  $summary += ""
}

Add-Content `
  -Path $env:GITHUB_STEP_SUMMARY `
  -Value ($summary -join "`n")
