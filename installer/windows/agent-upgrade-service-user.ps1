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

    # Well-known SIDs, not names: account names are localized, and on a French
    # Windows BUILTIN\Administrators does not resolve (it is "Administrateurs").
    # S-1-5-18 is SYSTEM, S-1-5-32-544 is BUILTIN\Administrators.
    $allowed = @()
    foreach ($wellKnownSid in @('S-1-5-18', 'S-1-5-32-544'))
    {
        $allowed += New-Object -TypeName System.Security.Principal.SecurityIdentifier -ArgumentList $wellKnownSid
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
    $content = $response.RawContentStream.ToArray()

    $signature = Get-HeaderValue -ResponseHeaders $response.Headers -Name $SignatureHeader
    if ([string]::IsNullOrEmpty($signature))
    {
        throw "The server returned no signature for this artifact, refusing to continue"
    }
    if (-not (Test-ArtifactSignature -Content $content -Base64Signature $signature))
    {
        throw "Signature does not match any trusted release key, refusing to continue"
    }

    [IO.File]::WriteAllBytes($DestinationPath, $content)
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
    # Caught here, not left to the caller: a failed write is a terminating error
    # whatever $ErrorActionPreference says, and the caller's catch would turn it
    # into a failed install.
    try
    {
        if ([string]::IsNullOrEmpty($InstallDirectory) -or (-not (Test-Path -Path $InstallDirectory)))
        {
            throw "The install directory does not exist"
        }
        Set-Content -Path (Join-Path -Path $InstallDirectory -ChildPath "openaev-agent.version") -Value $Version -NoNewline -ErrorAction Stop
    }
    catch
    {
        Write-Output "Could not record the installed version, the next upgrade will have no baseline to compare against."
    }
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

function ConvertTo-SafeUserName {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserName
    )
    $UserName = $UserName.ToLower()
    $pattern = '[\/\\:\*\?<>\|]'
    return ($UserName -replace $pattern, '')
}

if ([string]::IsNullOrEmpty($architecture)) { throw "Architecture $env:PROCESSOR_ARCHITECTURE is not supported yet, please create a ticket in openaev github project" }

$BasePath = "${OPENAEV_INSTALL_DIR}";
$User = whoami;
$SanitizedUser = ConvertTo-SafeUserName -UserName $user;
$ServiceName = "${OPENAEV_SERVICE_NAME}";
$AgentName = "$ServiceName-$SanitizedUser";

if ($BasePath -match "\\$ServiceName-[^\\]+$" -or $BasePath -match "/$ServiceName-[^/]+$") {
    $InstallDir = $BasePath
} else {
    if (-not $BasePath.EndsWith('\') -and -not $BasePath.EndsWith('/')) {
        $BasePath += '\'
    }
    $InstallDir = $BasePath + $AgentName
}

$AgentPath = $InstallDir + "\openaev-agent.exe";
$AgentUpgradedPath = $InstallDir + "\openaev-agent_upgrade.exe";
$AgentPreviousPath = $InstallDir + "\openaev-agent_previous.exe";

$stagingDirectory = New-ProtectedStagingDirectory
if (-not $stagingDirectory) { throw "Could not create a protected staging directory, refusing to continue" }
# Cleaned up in finally, so a download refused by the verification or a
# downgrade does not leave its staging directory behind in ProgramData.
$restartService = $false
try
{
    $downloadPath = Join-Path -Path $stagingDirectory -ChildPath "openaev-agent_upgrade.exe"
    $releaseVersion = Save-VerifiedArtifact -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/executable/openaev/windows/${architecture}" -RequestHeaders @{ "Authorization" = "Bearer ${OPENAEV_TOKEN}" } -DestinationPath $downloadPath
    Assert-NotADowngrade -InstallDirectory $InstallDir -Candidate $releaseVersion
    # Every file operation up to the baseline is terminating: this script does
    # not set $ErrorActionPreference, so a failure would only be reported and
    # the release recorded anyway, blocking the next attempt as a downgrade
    # while the old binary still runs. If this first move failed, the one below
    # would also install whatever stale, unverified openaev-agent_upgrade.exe
    # an earlier attempt left behind.
    Move-Item -Force $downloadPath $AgentUpgradedPath -ErrorAction Stop;

    sc.exe stop $AgentName;
    $restartService = $true
    # sc.exe only asks the service to stop. Until it is Stopped and its process
    # has exited, the binary is still in use and the restart below would find
    # the service still running.
    (Get-Service -Name $AgentName -ErrorAction Stop).WaitForStatus('Stopped', [TimeSpan]::FromSeconds(60))
    Get-Process | Where-Object { $_.Path -eq $AgentPath } | Wait-Process -Timeout 60 -ErrorAction Stop

    # Set aside rather than deleted, so a replacement that fails can put it back
    # for the restart in finally. -Force replaces one left by an earlier attempt.
    Move-Item -Force $AgentPath $AgentPreviousPath -ErrorAction Stop;
    try
    {
        Move-Item $AgentUpgradedPath $AgentPath -ErrorAction Stop;
    }
    catch
    {
        Move-Item $AgentPreviousPath $AgentPath -ErrorAction Continue;
        throw
    }
    Save-ReleaseVersion -InstallDirectory $InstallDir -Version $releaseVersion
    # Best effort: one left behind is replaced by the next upgrade.
    Remove-Item -Force $AgentPreviousPath -ErrorAction SilentlyContinue;
}
catch
{
    # Without a catch, a cmdlet error such as a failed download only ends this
    # try: the script would carry on after it and exit 0.
    throw
}
finally
{
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $stagingDirectory;
    # In finally, so an upgrade that failed after the stop does not leave the
    # agent stopped: by then the previous binary is still, or again, in place.
    if ($restartService) { sc.exe start $AgentName; }
}