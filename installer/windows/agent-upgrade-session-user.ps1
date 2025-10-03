[Net.ServicePointManager]::SecurityProtocol += [Net.SecurityProtocolType]::Tls12;
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
function Sanitize-UserName {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserName
    )
    $UserName = $UserName.ToLower()
    $pattern = '[\/\\:\*\?<>\|]'
    return ($UserName -replace $pattern, '')
}
$BasePath = "${OPENAEV_INSTALL_DIR}";
$User = whoami;
$SanitizedUser =  Sanitize-UserName -UserName $user;
$isElevated = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($isElevated) {
    $AgentName = "${OPENAEV_SERVICE_NAME}-Administrator-$SanitizedUser"
} else {
    $AgentName = "${OPENAEV_SERVICE_NAME}-$SanitizedUser"
}

if ($BasePath -like "*$AgentName*") {
    $CleanBasePath = $BasePath -replace [regex]::Escape("\$AgentName"), ""
    $CleanBasePath = $CleanBasePath -replace [regex]::Escape("/$AgentName"), ""
    $CleanBasePath = $CleanBasePath.TrimEnd('\', '/')
    $InstallDir = $BasePath
} else {
    $CleanBasePath = $BasePath
    $InstallDir = $BasePath + "\" + $AgentName
}

$AgentPath = $InstallDir + "\openaev-agent.exe";



# Manage the renaming OpenBAS -> OpenAEV ...
if(Test-Path "${OPENAEV_INSTALL_DIR}")
{
# Upgrade the agent if the folder *openaev* exists
Get-Process | Where-Object { $_.Path -eq "$AgentPath" } | Stop-Process -Force;
Invoke-WebRequest -Uri "${OPENAEV_URL}/api/agent/package/openaev/windows/${architecture}/session-user" -OutFile "openaev-installer-session-user.exe";
./openaev-installer-session-user.exe /S ~OPENAEV_URL="${OPENAEV_URL}" ~ACCESS_TOKEN="${OPENAEV_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENAEV_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENAEV_WITH_PROXY} ~SERVICE_NAME="${OPENAEV_SERVICE_NAME}" ~INSTALL_DIR="$CleanBasePath";
}
else
{
# Uninstall the old named agent *openbas* and install the new named agent *openaev* if the folder openaev doesn't exist
$AgentPath = $AgentPath -replace "openaev", "openbas"
$AgentPath = $AgentPath -replace "OAEV", "OBAS"
Get-Process | Where-Object { $_.Path -eq "$AgentPath" } | Stop-Process -Force;
$UninstallDir = "${OPENAEV_INSTALL_DIR}" -replace "openaev", "openbas"
& "${UninstallDir}/uninstall.exe" /S | Out-Null
Start-Sleep -Seconds 1
iex (iwr "${OPENAEV_URL}/api/agent/installer/openaev/windows/session-user/${OPENAEV_TOKEN}").Content
}
Start-Sleep -Seconds 1
rm -force ./openaev-installer-session-user.exe;