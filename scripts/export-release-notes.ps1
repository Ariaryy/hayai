param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [string]$ChangelogPath = (Join-Path $PSScriptRoot "../CHANGELOG.md")
)

$ErrorActionPreference = "Stop"

if ($Version -eq "Unreleased") {
    throw "Release notes must use a versioned changelog entry, not Unreleased"
}
$changelog = Get-Content -LiteralPath $ChangelogPath -Raw -Encoding utf8
$heading = [regex]::Escape($Version)
$entries = [regex]::Matches(
    $changelog,
    "(?ms)^## $heading - \d{4}-\d{2}-\d{2}[ \t]*\r?\n(?<notes>.*?)(?=^## |\z)"
)
if ($entries.Count -ne 1) {
    throw "Expected exactly one dated CHANGELOG.md entry for $Version; found $($entries.Count)"
}
$notes = $entries[0].Groups["notes"].Value.Trim()
if (-not $notes) {
    throw "CHANGELOG.md entry for $Version is empty"
}
Set-Content -LiteralPath $OutputPath -Value $notes -Encoding utf8
