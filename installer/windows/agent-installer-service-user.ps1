param(
    [Parameter(Mandatory=$true)]
    [string]$User,

    [Parameter(Mandatory=$true)]
    [string]$Password
)

[Net.ServicePointManager]::SecurityProtocol += [Net.SecurityProtocolType]::Tls12;
# Without this a failed download or a failed verification is only written to
# the output and the script still exits 0, so the caller cannot tell.
$ErrorActionPreference = 'Stop'
$script:installationFailed = $false

# --- Release integrity ------------------------------------------------------
# The trust anchor is the certificate list below, shipped inside this script.
# Nothing here asks the server what to trust, which is the whole point: an
# attacker able to serve a tampered binary could serve a tampered digest too.
#
# RSA with SHA-256, because Windows PowerShell 5.1 runs on .NET Framework and
# has no Ed25519. More than one certificate can be listed so a signing key can
# be rotated without a flag day: add the next one a release before it starts
# signing, drop the retired one a release after it stops.
$TrustedReleaseCertificates = @(
    'PLACEHOLDER_RELEASE_CERTIFICATE_1'
)

function New-ProtectedStagingDirectory
{
    [CmdletBinding(SupportsShouldProcess)]
    param()

    # Staging in the caller's working directory would leave predictable,
    # user-writable paths: an unprivileged local process could swap the
    # executable between the digest check and the call, and the verification
    # would prove nothing.
    $path = Join-Path -Path $env:ProgramData -ChildPath ("openaev-stage-" + [Guid]::NewGuid().ToString('N'))
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

    $allowed = @()
    foreach ($account in @('NT AUTHORITY\SYSTEM', 'BUILTIN\Administrators'))
    {
        $identity = New-Object -TypeName System.Security.Principal.NTAccount -ArgumentList $account
        $allowed += $identity.Translate([System.Security.Principal.SecurityIdentifier])
    }
    # The identity running this script. The session user flows are not elevated,
    # so leaving it out would lock the caller out of its own staging directory.
    $allowed += [System.Security.Principal.WindowsIdentity]::GetCurrent().User

    foreach ($sid in $allowed)
    {
        $accessRule = New-Object -TypeName System.Security.AccessControl.FileSystemAccessRule -ArgumentList @(
            $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($accessRule)
    }

    # Explicitly terminating: these scripts do not all set $ErrorActionPreference,
    # and a staging directory that kept its inherited permissions would silently
    # give up the protection this function exists for.
    Set-Acl -Path $directory.FullName -AclObject $acl -ErrorAction Stop

    # For the instant between creation and the line above, the directory still
    # inherits the ACL of ProgramData. The name is a fresh GUID so nothing can
    # target it, but checking that it is empty removes the question.
    if (Get-ChildItem -Path $directory.FullName -Force)
    {
        throw "Staging directory is not empty, refusing to continue"
    }

    return $directory.FullName
}

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

function Get-ManifestVersion
{
    param(
        [Parameter(Mandatory = $true)][string] $ManifestPath
    )
    foreach ($line in [IO.File]::ReadAllLines($ManifestPath))
    {
        $parts = $line.Trim() -split '\s+', 2
        if ($parts.Count -eq 2 -and $parts[0] -eq 'version')
        {
            return $parts[1].Trim()
        }
    }
    return $null
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

# Fetches the manifest, checks its signature, then checks the artifact digest.
# Returns the release version, throws otherwise.
function Invoke-ReleaseVerification
{
    param(
        [Parameter(Mandatory = $true)][string] $BaseUrl,
        [Parameter(Mandatory = $true)][string] $TenantId,
        [Parameter(Mandatory = $true)][hashtable] $Headers,
        [Parameter(Mandatory = $true)][string] $StagingDirectory,
        [Parameter(Mandatory = $true)][string] $Artifact,
        [Parameter(Mandatory = $true)][string] $FilePath
    )
    $manifestPath = Join-Path -Path $StagingDirectory -ChildPath "openaev-manifest"
    $signaturePath = Join-Path -Path $StagingDirectory -ChildPath "openaev-manifest.sig"

    Invoke-WebRequest -Uri "${BaseUrl}/api/tenants/${TenantId}/agent/manifest" -Headers $Headers -OutFile $manifestPath
    Invoke-WebRequest -Uri "${BaseUrl}/api/tenants/${TenantId}/agent/manifest.sig" -Headers $Headers -OutFile $signaturePath

    # The manifest is only worth what its signature is worth, so that comes first.
    if (-not (Test-ManifestSignature -ManifestPath $manifestPath -SignaturePath $signaturePath))
    {
        throw "Release manifest signature is not valid, refusing to continue"
    }

    $version = Get-ManifestVersion -ManifestPath $manifestPath
    if ([string]::IsNullOrEmpty($version))
    {
        throw "Release manifest carries no version, refusing to continue"
    }

    $expected = Get-ExpectedDigest -ManifestPath $manifestPath -Artifact $Artifact
    if ([string]::IsNullOrEmpty($expected))
    {
        throw "No entry for ${Artifact} in the release manifest"
    }

    $actual = (Get-FileHash -Path $FilePath -Algorithm SHA256).Hash
    if ($actual.ToLowerInvariant() -ne $expected.ToLowerInvariant())
    {
        throw "${Artifact} does not match the release manifest, refusing to continue"
    }

    return $version
}

# A signature says an artifact is ours, not that it is the current one. Without
# this an attacker could serve an older release, genuinely signed, whose
# weaknesses are already public. A machine with no recorded version has no
# baseline, which is the only way through the transition.
function Assert-NotADowngrade
{
    param(
        [Parameter(Mandatory = $true)][string] $InstallDirectory,
        [Parameter(Mandatory = $true)][string] $Candidate
    )
    if ([string]::IsNullOrEmpty($InstallDirectory)) { return }
    $recorded = Join-Path -Path $InstallDirectory -ChildPath "openaev-agent.version"
    if (-not (Test-Path -Path $recorded)) { return }
    $installed = (Get-Content -Path $recorded -Raw).Trim()
    if ([string]::IsNullOrEmpty($installed)) { return }

    $parsedCandidate = $null
    $parsedInstalled = $null
    if ((-not [version]::TryParse($Candidate, [ref]$parsedCandidate)) -or (-not [version]::TryParse($installed, [ref]$parsedInstalled)))
    {
        throw "Cannot compare release ${Candidate} with the installed ${installed}, refusing to continue"
    }
    if ($parsedCandidate -lt $parsedInstalled)
    {
        throw "Release ${Candidate} is older than the installed ${installed}, refusing to downgrade"
    }
}

function Save-ReleaseVersion
{
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory = $true)][string] $InstallDirectory,
        [Parameter(Mandatory = $true)][string] $Version
    )
    # Guarded: failing to record the version must not report a successful
    # install as failed. The upgrade path treats a missing file as "no baseline".
    if ([string]::IsNullOrEmpty($InstallDirectory) -or (-not (Test-Path -Path $InstallDirectory)))
    {
        Write-Output "Could not record the installed version, the next upgrade will have no baseline to compare against."
        return
    }
    $target = Join-Path -Path $InstallDirectory -ChildPath "openaev-agent.version"
    if ($PSCmdlet.ShouldProcess($target, 'Record the installed release version'))
    {
        Set-Content -Path $target -Value $Version -NoNewline
    }
}
# ----------------------------------------------------------------------------
$isElevatedPowershell = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($isElevatedPowershell -like "False") { throw "PowerShell 'Run as Administrator' is required for installation" }

#Check that $User is in domain\username format
if ($User -notmatch '^[^\\]+\\[^\\]+$') {
    throw "User must be in the format 'DOMAIN\Username'. Provided: '$User'"
}
# Disallow '.' as domain
$parts  = $User -split '\\', 2
$domain = $parts[0]
$username = $parts[1]
if ($domain -eq '.') {
    throw "Local user notation '.' is not allowed. Please specify a 'DOMAIN\Username'."
}
#Verify the account actually exists by translating to a SID.
try {
    $userSID = ([System.Security.Principal.NTAccount] $User).Translate([System.Security.Principal.SecurityIdentifier])
}
catch {
    throw "The user '$User' does not exist or could not be found."
}

# Resolve the user's home directory
try {
    # Get the user's profile path from the registry using their SID
    $profilePath = (Get-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$($userSID.Value)" -Name ProfileImagePath).ProfileImagePath

    # If registry lookup fails, try to construct it
    if ([string]::IsNullOrEmpty($profilePath)) {
        # For domain users, the profile is typically under C:\Users\username
        # For local users, it's the same pattern
        $profilePath = "C:\Users\$username"
    }
} catch {
    # Fallback to constructing the path if registry lookup fails
    $profilePath = "C:\Users\$username"
}

# Construct the full installation directory path
$installDir = "${OPENAEV_INSTALL_DIR}"
if ($installDir -like ".\*" -or $installDir -like ".\*") {
    # Remove leading .\ or ./ if present
    $installDir = $installDir -replace '^\.[\\/]', ''
}

# Combine the profile path with the install directory
$fullInstallPath = Join-Path $profilePath $installDir

Write-Output "Resolved installation path: $fullInstallPath"

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
Write-Output "Downloading and installing OpenAEV Agent..."
try {
    $stagingDirectory = New-ProtectedStagingDirectory
    if (-not $stagingDirectory) { throw "Could not create a protected staging directory, refusing to continue" }
    $downloadPath = Join-Path -Path $stagingDirectory -ChildPath "agent-installer-service-user.exe"
    Invoke-WebRequest -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/package/openaev/windows/${architecture}/service-user" -Headers @{ "Authorization" = "Bearer ${OPENAEV_TOKEN}" } -OutFile $downloadPath;
$releaseVersion = Invoke-ReleaseVerification -BaseUrl "${OPENAEV_URL}" -TenantId "${OPENAEV_TENANT_ID}" -Headers @{ "Authorization" = "Bearer ${OPENAEV_TOKEN}" } -StagingDirectory $stagingDirectory -Artifact "agent/package/openaev/windows/${architecture}/service-user" -FilePath $downloadPath
    # Use the resolved full installation path
    & $downloadPath /S ~OPENAEV_URL="${OPENAEV_URL}" ~ACCESS_TOKEN="${OPENAEV_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENAEV_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENAEV_WITH_PROXY} ~SERVICE_NAME="${OPENAEV_SERVICE_NAME}" ~INSTALL_DIR="$fullInstallPath" ~TENANT_ID="${OPENAEV_TENANT_ID}" ~USER="$User" ~PASSWORD="$Password" | Out-Null;
    # $ErrorActionPreference does not apply to native executables in Windows
    # PowerShell 5.1, so a failing installer has to be caught explicitly.
    if ($LASTEXITCODE -ne 0)
    {
        throw "Agent installer exited with code ${LASTEXITCODE}"
    }
    # Only now: recording a release that failed to install would make the
    # next attempt look like a downgrade and block it.
    Save-ReleaseVersion -InstallDirectory "$fullInstallPath" -Version $releaseVersion
    Write-Output "OpenAEV agent has been successfully installed"
} catch {
    Write-Output "Installation failed"
    Write-Output "Note: PowerShell 7 or higher is recommended. If the issue persists, consider upgrading."
    Write-Output $_
    $script:installationFailed = $true
} finally {
    Start-Sleep -Seconds 2
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $stagingDirectory;
  	if ($location -like "*C:\Windows\System32*") { Set-Location C:\Windows\System32 }
}

if ($script:installationFailed) { exit 1 }
