param(
    [Parameter(Mandatory=$true)]
    [string]$User,

    [Parameter(Mandatory=$true)]
    [string]$Password
)

[Net.ServicePointManager]::SecurityProtocol += [Net.SecurityProtocolType]::Tls12;
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
$installDir = "${OPENBAS_INSTALL_DIR}"
if ($installDir -like ".\*" -or $installDir -like ".\*") {
    # Remove leading .\ or ./ if present
    $installDir = $installDir -replace '^\.[\\/]', ''
}

# Combine the profile path with the install directory
$fullInstallPath = Join-Path $profilePath $installDir

echo "Resolved installation path: $fullInstallPath"

# Can't install the OpenBAS agent in System32 location because NSIS 64 exe
$location = Get-Location
if ($location -like "*C:\Windows\System32*") { cd C:\ }
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
if ([string]::IsNullOrEmpty($architecture)) { throw "Architecture $env:PROCESSOR_ARCHITECTURE is not supported yet, please create a ticket in openbas github project" }
echo "Downloading and installing OpenBAS Agent..."
try {
    Invoke-WebRequest -Uri "${OPENBAS_URL}/api/agent/package/openbas/windows/${architecture}/service-user" -OutFile "agent-installer-service-user.exe";
    # Use the resolved full installation path
    ./agent-installer-service-user.exe /S ~OPENBAS_URL="${OPENBAS_URL}" ~ACCESS_TOKEN="${OPENBAS_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENBAS_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENBAS_WITH_PROXY} ~SERVICE_NAME="${OPENBAS_SERVICE_NAME}" ~INSTALL_DIR="$fullInstallPath" ~USER="$User" ~PASSWORD="$Password" | Out-Null;
    echo "OpenBAS agent has been successfully installed"
} catch {
    echo "Installation failed"
    if ((Get-Host).Version.Major -lt 7) { throw "PowerShell 7 or higher is required for installation" }
    else { echo $_ }
} finally {
    rm -force ./agent-installer-service-user.exe;
  	if ($location -like "*C:\Windows\System32*") { cd C:\Windows\System32 }
}