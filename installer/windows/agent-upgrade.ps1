[Net.ServicePointManager]::SecurityProtocol += [Net.SecurityProtocolType]::Tls12;

# --- Release integrity ------------------------------------------------------
# The platform returns a detached signature of the artifact it just served, in a
# response header. It is checked against the public key list below, shipped
# inside this script. Nothing here asks the server what to trust, which is the
# point: an attacker able to serve a tampered binary could serve a tampered
# reference too.
#
# RSA with SHA-256, because Windows PowerShell 5.1 runs on .NET Framework and
# has no Ed25519. More than one key can be listed so a signing key can be
# rotated without a flag day.
#
# Each key is the public key the Linux and macOS scripts embed as PEM, written
# as RSAKeyValue XML: RSACryptoServiceProvider imports that on both .NET
# Framework and .NET, while neither PEM nor a certificate's PublicKey.Key works
# on both.
$SignatureHeader = 'X-Signature-Sha256-Rsa'
$VersionHeader = 'X-Release-Version'
$TrustedReleaseKeys = @(
    '<RSAKeyValue><Modulus>ku5It9odazO7lvbj1ph6yd7q3zCr3A7lfjxIGmioBgz+lSLPX5jonnnHE2QCX/vCBZWMRM5EYc/RyVz3a9CBbUYhQp0sPJZJJEWG1IrgE+KlH0T0i/CumsCIpw6J0/E51l2hamWAR54v5xg8bUD9R9kUEDQBCNPo6trJrmVAyZSU+9STZVfLjBPj90eB3BAzg+W0WopJJwk0u2Kn09KSt118xWpRsoPfO/n8AeU5k5swQsDy3gN/gUdG7oEoprB1nYY+aU7KBWzExjWL7etgkjjC1rAUj2duPyug1FIbRKeoMN6S8fCHdD0Xn2sfxFkF3UlfGbR3lmnzGl8qbs0lJQ==</Modulus><Exponent>AQAB</Exponent></RSAKeyValue>'
)

function New-ProtectedStagingDirectory
{
    [CmdletBinding(SupportsShouldProcess)]
    param()

    # Staging in the caller's working directory would leave predictable,
    # user-writable paths: an unprivileged local process could swap the
    # executable between verification and the call.
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

function Get-HeaderValue
{
    param(
        [Parameter(Mandatory = $true)] $ResponseHeaders,
        [Parameter(Mandatory = $true)][string] $Name
    )
    foreach ($key in $ResponseHeaders.Keys)
    {
        if ($key -ieq $Name)
        {
            $value = $ResponseHeaders[$key]
            if ($value -is [array])
            {
                return ($value | Select-Object -First 1)
            }
            return $value
        }
    }
    return $null
}

function Test-ArtifactSignature
{
    param(
        [Parameter(Mandatory = $true)][byte[]] $Content,
        [Parameter(Mandatory = $true)][string] $Base64Signature
    )
    $signature = [Convert]::FromBase64String($Base64Signature)
    foreach ($keyXml in $TrustedReleaseKeys)
    {
        try
        {
            $key = New-Object System.Security.Cryptography.RSACryptoServiceProvider
            $key.FromXmlString($keyXml)
            if ($key.VerifyData($Content, 'SHA256', $signature))
            {
                return $true
            }
        }
        catch
        {
            # A key that will not load, or that did not sign this artifact, is
            # not an error: the next one may still validate it.
            Write-Verbose "Release key did not validate the artifact: $_"
        }
    }
    return $false
}

# Downloads an artifact, refuses it unless the server signed it, and only then
# writes it to disk. A missing signature is a hard failure, never a warning:
# whoever can replace an artifact can also strip the header that would have
# given them away. Returns the release version when the server sends one.
function Save-VerifiedArtifact
{
    param(
        [Parameter(Mandatory = $true)][string] $Uri,
        [Parameter(Mandatory = $true)][hashtable] $RequestHeaders,
        [Parameter(Mandatory = $true)][string] $DestinationPath
    )
    $response = Invoke-WebRequest -Uri $Uri -Headers $RequestHeaders -UseBasicParsing

    $signature = Get-HeaderValue -ResponseHeaders $response.Headers -Name $SignatureHeader
    if ([string]::IsNullOrEmpty($signature))
    {
        throw "The server returned no signature for this artifact, refusing to continue"
    }
    if (-not (Test-ArtifactSignature -Content $response.Content -Base64Signature $signature))
    {
        throw "Signature does not match any trusted release key, refusing to continue"
    }

    [IO.File]::WriteAllBytes($DestinationPath, $response.Content)
    return (Get-HeaderValue -ResponseHeaders $response.Headers -Name $VersionHeader)
}

# A signature says an artifact is ours, not that it is the current one. Without
# a version an attacker could serve an older release, genuinely signed, whose
# weaknesses are already public.
#
# The platform does not send this header yet, so its absence only disables the
# downgrade check rather than failing the install. Making it trustworthy means
# binding the version into what is signed, otherwise stripping the header is
# enough to disable the check.
function Assert-NotADowngrade
{
    param(
        [Parameter(Mandatory = $true)][AllowEmptyString()][string] $InstallDirectory,
        [Parameter(Mandatory = $true)][AllowEmptyString()][AllowNull()][string] $Candidate
    )
    if ([string]::IsNullOrEmpty($Candidate)) { return }
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
    param(
        [Parameter(Mandatory = $true)][AllowEmptyString()][string] $InstallDirectory,
        [Parameter(Mandatory = $true)][AllowEmptyString()][AllowNull()][string] $Version
    )
    if ([string]::IsNullOrEmpty($Version)) { return }
    # Guarded: failing to record the version must not report a successful
    # install as failed. The upgrade path treats a missing file as "no baseline".
    if ([string]::IsNullOrEmpty($InstallDirectory) -or (-not (Test-Path -Path $InstallDirectory)))
    {
        Write-Output "Could not record the installed version, the next upgrade will have no baseline to compare against."
        return
    }
    Set-Content -Path (Join-Path -Path $InstallDirectory -ChildPath "openaev-agent.version") -Value $Version -NoNewline
}
# ----------------------------------------------------------------------------
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

# Manage the renaming OpenBAS -> OpenAEV ...
$OpenAEVPath = "${OPENAEV_INSTALL_DIR}" -replace "openbas", "openaev"
$OpenAEVPath = "$OpenAEVPath" -replace "OBAS", "OAEV"
if(Test-Path "$OpenAEVPath")
{
# Upgrade the agent if the folder *OAEV* exists
$stagingDirectory = New-ProtectedStagingDirectory
if (-not $stagingDirectory) { throw "Could not create a protected staging directory, refusing to continue" }
$downloadPath = Join-Path -Path $stagingDirectory -ChildPath "openaev-installer.exe"
$releaseVersion = Save-VerifiedArtifact -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/package/openaev/windows/${architecture}/service" -RequestHeaders @{ "Authorization" = "Bearer ${OPENAEV_TOKEN}" } -DestinationPath $downloadPath
Assert-NotADowngrade -InstallDirectory "${OPENAEV_INSTALL_DIR}" -Candidate $releaseVersion
& $downloadPath /S ~OPENAEV_URL="${OPENAEV_URL}" ~ACCESS_TOKEN="${OPENAEV_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENAEV_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENAEV_WITH_PROXY} ~SERVICE_NAME="${OPENAEV_SERVICE_NAME}" ~INSTALL_DIR="${OPENAEV_INSTALL_DIR}" ~TENANT_ID="${OPENAEV_TENANT_ID}" | Out-Null;
# $ErrorActionPreference does not apply to native executables in Windows
# PowerShell 5.1, so a failing installer has to be caught explicitly.
if ($LASTEXITCODE -ne 0)
{
    throw "Agent installer exited with code ${LASTEXITCODE}"
}
# Only now: recording a release that failed to install would make the next
# attempt look like a downgrade and block it.
Save-ReleaseVersion -InstallDirectory "${OPENAEV_INSTALL_DIR}" -Version $releaseVersion
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $stagingDirectory;
}
else
{
# Uninstall the old named agent *OBAS* and install the new named agent *OAEV* if the folder OAEV doesn't exist
$installationDir=[System.Uri]::EscapeDataString("$OpenAEVPath")
$OpenAEVService = "${OPENAEV_SERVICE_NAME}" -replace "openbas", "openaev"
$OpenAEVService = "$OpenAEVService" -replace "OBAS", "OAEV"
$serviceName=[System.Uri]::EscapeDataString("$OpenAEVService")
Invoke-WebRequest -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/installer/openaev/windows/service/${OPENAEV_TOKEN}?installationDir=$installationDir&amp;serviceName=$serviceName" -Headers @{ "Authorization" = "Bearer ${OPENAEV_TOKEN}" } -OutFile "openaev-installer.ps1";
./openaev-installer.ps1
sc.exe stop "${OPENAEV_SERVICE_NAME}"
$UninstallDir = "${OPENAEV_INSTALL_DIR}" -replace "openaev", "openbas"
$UninstallDir = "${OPENAEV_INSTALL_DIR}" -replace "OAEV", "OBAS"
Remove-Item -Force "${UninstallDir}/openbas.ico"
Remove-Item -Force "${UninstallDir}/openbas_agent_kill.ps1"
Remove-Item -Force "${UninstallDir}/openbas-agent.exe"
Remove-Item -Force "${UninstallDir}/openbas-agent-config.toml"
Remove-Item -Force "${UninstallDir}/uninstall.exe"
sc.exe delete "${OPENAEV_SERVICE_NAME}"
Remove-Item -Force ./openaev-installer.ps1
}
