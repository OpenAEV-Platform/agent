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

function New-ProtectedStagingDirectory
{
    [CmdletBinding(SupportsShouldProcess)]
    param()

    # Staging in the caller's working directory would leave predictable,
    # user-writable paths: an unprivileged local process could swap the
    # executable between the digest check and the call, and the verification
    # would prove nothing. This directory is unguessable and reachable only by
    # SYSTEM and Administrators.
    $path = Join-Path -Path $env:ProgramData -ChildPath ("openaev-install-" + [Guid]::NewGuid().ToString('N'))
    if (-not $PSCmdlet.ShouldProcess($path, 'Create protected staging directory'))
    {
        return $null
    }

    $directory = New-Item -ItemType Directory -Path $path -Force

    $acl = Get-Acl -Path $directory.FullName
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($rule in @($acl.Access))
    {
        [void]$acl.RemoveAccessRule($rule)
    }
    foreach ($account in @('NT AUTHORITY\SYSTEM', 'BUILTIN\Administrators'))
    {
        $identity = New-Object -TypeName System.Security.Principal.NTAccount -ArgumentList $account
        $sid = $identity.Translate([System.Security.Principal.SecurityIdentifier])
        $accessRule = New-Object -TypeName System.Security.AccessControl.FileSystemAccessRule -ArgumentList @(
            $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($accessRule)
    }
    Set-Acl -Path $directory.FullName -AclObject $acl

    # For the instant between creation and the line above, the directory still
    # inherits the ACL of ProgramData, where unprivileged users can create
    # files. The name is a fresh GUID so nothing can target it, but checking
    # that it is empty costs nothing and removes the question.
    if (Get-ChildItem -Path $directory.FullName -Force)
    {
        throw "Staging directory is not empty, refusing to install"
    }

    return $directory.FullName
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
$staging = $null

Write-Output "Downloading and installing OpenAEV Agent..."
try {
    $staging = New-ProtectedStagingDirectory
    if (-not $staging)
    {
        throw "Could not create a protected staging directory, refusing to install"
    }
    $installerPath = Join-Path $staging "openaev-installer.exe"
    $manifestPath = Join-Path $staging "openaev-manifest"
    $signaturePath = Join-Path $staging "openaev-manifest.sig"

    Invoke-WebRequest -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/package/openaev/windows/${architecture}/service" -Headers $headers -OutFile $installerPath;
    Invoke-WebRequest -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/manifest" -Headers $headers -OutFile $manifestPath;
    Invoke-WebRequest -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/manifest.sig" -Headers $headers -OutFile $signaturePath;

    # From here on everything is local. The manifest is only worth what its
    # signature is worth, so that is checked first.
    if (-not (Test-ManifestSignature -ManifestPath $manifestPath -SignaturePath $signaturePath))
    {
        throw "Release manifest signature is not valid, refusing to install"
    }

    $expected = Get-ExpectedDigest -ManifestPath $manifestPath -Artifact $artifact
    if ([string]::IsNullOrEmpty($expected))
    {
        throw "No entry for ${artifact} in the release manifest, refusing to install"
    }

    $actual = (Get-FileHash -Path $installerPath -Algorithm SHA256).Hash
    if ($actual.ToLowerInvariant() -ne $expected.ToLowerInvariant())
    {
        throw "Agent installer does not match the release manifest, refusing to install"
    }
    Write-Output "Signature and digest verified."

    & $installerPath /S ~OPENAEV_URL="${OPENAEV_URL}" ~ACCESS_TOKEN="${OPENAEV_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENAEV_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENAEV_WITH_PROXY} ~SERVICE_NAME="${OPENAEV_SERVICE_NAME}" ~INSTALL_DIR="${OPENAEV_INSTALL_DIR}" ~TENANT_ID="${OPENAEV_TENANT_ID}"  | Out-Null;
    # $ErrorActionPreference does not apply to native executables in Windows
    # PowerShell 5.1, so a failing installer has to be caught explicitly.
    if ($LASTEXITCODE -ne 0)
    {
        throw "Agent installer exited with code ${LASTEXITCODE}"
    }
	Write-Output "OpenAEV agent has been successfully installed"
} catch {
    Write-Output "Installation failed"
    Write-Output "Note: PowerShell 7 or higher is recommended. If the issue persists, consider upgrading."
    Write-Output $_
    $script:installationFailed = $true
} finally {
    Start-Sleep -Seconds 2
    if ($staging) { Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $staging }
  	if ($location -like "*C:\Windows\System32*") { Set-Location C:\Windows\System32 }
}

if ($script:installationFailed) { exit 1 }
