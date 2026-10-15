$ErrorActionPreference = 'Stop'
if ($env:RELEASE_TAG -notmatch '^v\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$') { throw 'Invalid release tag.' }
$sha = git rev-parse "refs/tags/$($env:RELEASE_TAG)^{commit}"
if ($LASTEXITCODE -ne 0 -or -not $sha) { throw "Could not resolve $env:RELEASE_TAG to a commit." }
git fetch --no-tags origin '+refs/heads/main:refs/remotes/origin/main'
if ($LASTEXITCODE -ne 0) { throw 'Could not fetch origin/main.' }
git merge-base --is-ancestor $sha origin/main
if ($LASTEXITCODE -ne 0) {
    Write-Host "::error::$env:RELEASE_TAG is not on main"
    throw 'Release commit is not on main.'
}
$arguments = @('describe', '--tags', '--abbrev=0', '--match', 'v*', '--exclude', $env:RELEASE_TAG)
if (-not $env:RELEASE_TAG.Contains('-')) { $arguments += @('--exclude', '*-*') }
$previous = git @arguments $sha 2>$null
if ($LASTEXITCODE -ne 0) { $previous = '' }
$prerelease = $env:RELEASE_TAG.Contains('-')
@(
    "sha=$sha"
    "previous=$previous"
    "prerelease=$($prerelease.ToString().ToLowerInvariant())"
    "make_latest=$(((-not $prerelease).ToString()).ToLowerInvariant())"
) | Add-Content -LiteralPath $env:GITHUB_OUTPUT -Encoding utf8
$global:LASTEXITCODE = 0
