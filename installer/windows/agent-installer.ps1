[Net.ServicePointManager]::SecurityProtocol += [Net.SecurityProtocolType]::Tls12;
# Without this a failed download or a failed verification is only written to the
# output and the script still exits 0, so the caller cannot tell.
$ErrorActionPreference = 'Stop'

# Certificates accepted for release manifests, base64 DER. More than one can be
# listed so a signing key can be rotated without a flag day: a manifest is
# accepted as soon as one of them validates its signature. Add the next one a
# release before it starts signing, drop the retired one a release after.
# RSA with SHA-256, because Windows PowerShell 5.1 runs on .NET Framework and
# has no Ed25519.
$TrustedReleaseCertificates = @(
    'PLACEHOLDER_RELEASE_CERTIFICATE_1'
)

function Test-ManifestSignature
{
    param(
        [Parameter(Mandatory = $true)][string] $ManifestPath,
        [Parameter(Mandatory = $true)][string] $SignaturePath
    )
    $manifest = [IO.File]::ReadAllBytes($ManifestPath)
    $signature = [IO.File]::ReadAllBytes($SignaturePath)
    foreach ($encoded in $TrustedReleaseCertificates)
    {
        try
        {
            $der = [Convert]::FromBase64String($encoded)
            $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2(, $der)
            if ($cert.PublicKey.Key.VerifyData($manifest, 'SHA256', $signature))
            {
                return $true
            }
        }
        catch
        {
            # A certificate that will not load, or that did not sign this
            # manifest, is not an error: the next one may still validate it.
            Write-Verbose "Release certificate did not validate the manifest: $_"
        }
    }
    return $false
}

function Get-ExpectedDigest
{
    param(
        [Parameter(Mandatory = $true)][string] $ManifestPath,
        [Parameter(Mandatory = $true)][string] $Artifact
    )
    foreach ($line in [IO.File]::ReadAllLines($ManifestPath))
    {
        $parts = $line.Trim() -split '\s+', 2
        if ($parts.Count -eq 2 -and $parts[1].Trim() -eq $Artifact)
        {
            return $parts[0].Trim()
        }
    }
    return $null
}

$isElevatedPowershell = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($isElevatedPowershell -like "False") { throw "PowerShell 'Run as Administrator' is required for installation" }
# Can't install the OpenAEV agent in System32 location because NSIS 64 exe
$location = Get-Location
if ($location -like "*C:\Windows\System32*") { Set-Location C:\ }
switch ($env:PROCESSOR_ARCHITECTURE)
{
    "AMD64" {$architecture = "x86_64"; Break}
	"ARM64" {$architecture = "arm64"; Break}
	"x86" {
		switch ($env:PROCESSOR_ARCHITEW6432)
		{
			"AMD64" {$architecture = "x86_64"; Break}
			"ARM64" {$architecture = "arm64"; Break}
		}
	}
}
if ([string]::IsNullOrEmpty($architecture)) { throw "Architecture $env:PROCESSOR_ARCHITECTURE is not supported yet, please create a ticket in openaev github project" }

$script:installationFailed = $false
$headers = @{ "Authorization" = "Bearer ${OPENAEV_TOKEN}" }
$artifact = "agent/package/openaev/windows/${architecture}/service"

Write-Output "Downloading and installing OpenAEV Agent..."
try {
    Invoke-WebRequest -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/package/openaev/windows/${architecture}/service" -Headers $headers -OutFile "openaev-installer.exe";
    Invoke-WebRequest -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/manifest" -Headers $headers -OutFile "openaev-manifest";
    Invoke-WebRequest -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/manifest.sig" -Headers $headers -OutFile "openaev-manifest.sig";

    # From here on everything is local. The manifest is only worth what its
    # signature is worth, so that is checked first.
    if (-not (Test-ManifestSignature -ManifestPath "openaev-manifest" -SignaturePath "openaev-manifest.sig"))
    {
        throw "Release manifest signature is not valid, refusing to install"
    }

    $expected = Get-ExpectedDigest -ManifestPath "openaev-manifest" -Artifact $artifact
    if ([string]::IsNullOrEmpty($expected))
    {
        throw "No entry for ${artifact} in the release manifest, refusing to install"
    }

    $actual = (Get-FileHash -Path "openaev-installer.exe" -Algorithm SHA256).Hash
    if ($actual.ToLowerInvariant() -ne $expected.ToLowerInvariant())
    {
        throw "Agent installer does not match the release manifest, refusing to install"
    }
    Write-Output "Signature and digest verified."

    ./openaev-installer.exe /S ~OPENAEV_URL="${OPENAEV_URL}" ~ACCESS_TOKEN="${OPENAEV_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENAEV_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENAEV_WITH_PROXY} ~SERVICE_NAME="${OPENAEV_SERVICE_NAME}" ~INSTALL_DIR="${OPENAEV_INSTALL_DIR}" ~TENANT_ID="${OPENAEV_TENANT_ID}"  | Out-Null;
	Write-Output "OpenAEV agent has been successfully installed"
} catch {
    Write-Output "Installation failed"
    Write-Output "Note: PowerShell 7 or higher is recommended. If the issue persists, consider upgrading."
    Write-Output $_
    $script:installationFailed = $true
} finally {
    Start-Sleep -Seconds 2
    Remove-Item -Force -ErrorAction SilentlyContinue ./openaev-installer.exe, ./openaev-manifest, ./openaev-manifest.sig;
  	if ($location -like "*C:\Windows\System32*") { Set-Location C:\Windows\System32 }
}

if ($script:installationFailed) { exit 1 }
