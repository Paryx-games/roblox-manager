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

$script:vtBase = "https://www.virustotal.com/api/v3"
$script:vtHeaders = @{ "x-apikey" = $env:VT_API_KEY }

# free tier is 4 requests a minute across every call, lookups and polls included
$script:vtMinGapSeconds = 16
$script:vtLastCall = [DateTime]::MinValue

# stays well inside the 15 minute job timeout so a slow scan never fails the release
$script:vtDeadline = [DateTime]::UtcNow.AddMinutes(10)

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

function Get-VtTotal {
  param($Stats)

  $total = 0

  foreach ($property in $Stats.PSObject.Properties) {
    $total += [int]$property.Value
  }

  return $total
}

function New-VtStatsResult {
  param(
    [string]$Name,
    [string]$Hash,
    $Stats
  )

  $flagged = [int]$Stats.malicious + [int]$Stats.suspicious
  $total = Get-VtTotal $Stats

  return New-VtResult `
    -Name $Name `
    -Hash $Hash `
    -ReportUrl "https://www.virustotal.com/gui/file/$Hash" `
    -Flagged $flagged `
    -Total $total `
    -Message "$flagged / $total security vendors flagged this file"
}

function Invoke-Vt {
  param(
    [string]$Uri,
    [string]$Method = "Get",
    [hashtable]$Form = $null,
    [int]$TimeoutSec = 30,
    [switch]$AllowNotFound
  )

  for ($attempt = 1; $attempt -le 4; $attempt++) {
    if ([DateTime]::UtcNow -ge $script:vtDeadline) {
      throw "VirusTotal time budget ran out."
    }

    $wait = $script:vtMinGapSeconds -
      ([DateTime]::UtcNow - $script:vtLastCall).TotalSeconds

    if ($wait -gt 0) {
      Start-Sleep -Milliseconds ([int]($wait * 1000))
    }

    $script:vtLastCall = [DateTime]::UtcNow

    $params = @{
      Uri = $Uri
      Method = $Method
      Headers = $script:vtHeaders
      TimeoutSec = $TimeoutSec
    }

    if ($Form) {
      $params.Form = $Form
    }

    try {
      return Invoke-RestMethod @params
    } catch {
      $status = 0

      if ($_.Exception.Response) {
        $status = [int]$_.Exception.Response.StatusCode
      }

      # a 404 on a hash lookup just means nobody has scanned this build yet
      if ($status -eq 404 -and $AllowNotFound) {
        return $null
      }

      # status 0 covers timeouts and dropped connections
      $retryable = ($status -eq 0) -or ($status -eq 429) -or ($status -ge 500)

      if (-not $retryable -or $attempt -eq 4) {
        throw
      }

      $backoff = if ($status -eq 429) { 60 } else { 10 * $attempt }

      Write-Host "VirusTotal returned $status; retrying in ${backoff}s (attempt $attempt of 4)."
      Start-Sleep -Seconds $backoff
    }
  }
}

$resultsByName = @{}
$pending = @{}

# submit everything first so both scans queue while we poll
foreach ($file in $files) {
  $hash = $expected[$file.Name]
  $reportUrl = "https://www.virustotal.com/gui/file/$hash"

  if (-not $env:VT_API_KEY) {
    $resultsByName[$file.Name] = New-VtResult `
      -Name $file.Name `
      -Hash $hash `
      -Message "Scan unavailable for this build."

    continue
  }

  try {
    $known = Invoke-Vt `
      -Uri "$script:vtBase/files/$hash" `
      -AllowNotFound

    if ($known) {
      $stats = $known.data.attributes.last_analysis_stats

      if ($stats -and (Get-VtTotal $stats) -gt 0) {
        $resultsByName[$file.Name] = New-VtStatsResult `
          -Name $file.Name `
          -Hash $hash `
          -Stats $stats

        continue
      }
    }
  } catch {
    Write-Host "::warning::VirusTotal hash lookup for $($file.Name) failed: $($_.Exception.Message)"
  }

  try {
    $uploadUri = "$script:vtBase/files"

    # the plain upload endpoint rejects anything over 32 MB
    if ($file.Length -gt 32MB) {
      $uploadUri = (Invoke-Vt -Uri "$script:vtBase/files/upload_url").data
    }

    $upload = Invoke-Vt `
      -Uri $uploadUri `
      -Method Post `
      -Form @{ file = $file } `
      -TimeoutSec 180

    if (-not $upload.data.id) {
      throw "the upload response had no analysis id."
    }

    $pending[$file.Name] = $upload.data.id
  } catch {
    $resultsByName[$file.Name] = New-VtResult `
      -Name $file.Name `
      -Hash $hash `
      -ReportUrl $reportUrl `
      -Message "Scan unavailable: upload failed. $($_.Exception.Message)"
  }
}

while (
  $pending.Count -gt 0 -and
  [DateTime]::UtcNow -lt $script:vtDeadline
) {
  foreach ($name in @($pending.Keys)) {
    try {
      $analysis = Invoke-Vt `
        -Uri "$script:vtBase/analyses/$($pending[$name])"
    } catch {
      Write-Host "VirusTotal polling for $name failed: $($_.Exception.Message)"
      continue
    }

    if ($analysis.data.attributes.status -eq "completed") {
      $resultsByName[$name] = New-VtStatsResult `
        -Name $name `
        -Hash $expected[$name] `
        -Stats $analysis.data.attributes.stats

      $pending.Remove($name)
    }
  }
}

foreach ($name in @($pending.Keys)) {
  $resultsByName[$name] = New-VtResult `
    -Name $name `
    -Hash $expected[$name] `
    -ReportUrl "https://www.virustotal.com/gui/file/$($expected[$name])" `
    -Message (
      "Scan unavailable: VirusTotal did not " +
      "finish within the polling window."
    )
}

$results = @(
  foreach ($file in $files) {
    $resultsByName[$file.Name]
  }
)

foreach ($result in $results) {
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
  "VirusTotal detections are automated vendor signals, not proof that an asset is safe or malicious. Review the vendor, detection name, and linked report; do not assume a new or different detection is a false positive."
  ""
  '> [!WARNING]'
  '> **SecureAge detection on the installer.** SecureAge has flagged the installer since its introduction in v2.1.0. Based on this repeated history, we believe this specific SecureAge detection is a false positive. We have not seen SecureAge flag the portable executable. This history does not apply to other vendors or new detections.'
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