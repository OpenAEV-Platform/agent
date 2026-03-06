$ErrorActionPreference = 'Stop'

# Ensure NuGet provider is available (required for Install-Module on fresh machines / CI runners)
if (-not (Get-PackageProvider -Name NuGet -ListAvailable -ErrorAction SilentlyContinue)) {
    Install-PackageProvider -Name NuGet -MinimumVersion 2.8.5.201 -Force -Scope CurrentUser | Out-Null
}

# Install PSScriptAnalyzer if not already present, then load it
$analyzer = Get-Module -ListAvailable -Name PSScriptAnalyzer | Select-Object -First 1

if (-not $analyzer) {
    Write-Host "Installing PSScriptAnalyzer ..." -ForegroundColor Yellow
    Install-Module -Name PSScriptAnalyzer -Force -SkipPublisherCheck -Scope CurrentUser
}

Import-Module PSScriptAnalyzer -Force

# Lint all installer scripts
$settings = Join-Path $PSScriptRoot 'PSScriptAnalyzerSettings.psd1'
$results = Invoke-ScriptAnalyzer -Path "$PSScriptRoot\*.ps1" -Recurse -Settings $settings

if ($results) {
    $results | Format-Table -AutoSize
    Write-Host "$($results.Count) issue(s) found." -ForegroundColor Red
    exit 1
} else {
    Write-Host "No issues found." -ForegroundColor Green
}

