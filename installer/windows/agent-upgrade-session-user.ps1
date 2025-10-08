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
$OpenAEVPath = "${OPENAEV_INSTALL_DIR}" -replace "openbas", "openaev"
$OpenAEVPath = "$OpenAEVPath" -replace "OBAS", "OAEV"
if(Test-Path "$OpenAEVPath")
{
# Upgrade the agent if the folder *openaev* exists
Get-Process | Where-Object { $_.Path -eq "$AgentPath" } | Stop-Process -Force;
Invoke-WebRequest -Uri "${OPENAEV_URL}/api/agent/package/openaev/windows/${architecture}/session-user" -OutFile "openaev-installer-session-user.exe";
./openaev-installer-session-user.exe /S ~OPENAEV_URL="${OPENAEV_URL}" ~ACCESS_TOKEN="${OPENAEV_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENAEV_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENAEV_WITH_PROXY} ~SERVICE_NAME="${OPENAEV_SERVICE_NAME}" ~INSTALL_DIR="$CleanBasePath";
}
else
{
# Uninstall the old named agent *openbas* and install the new named agent *openaev* if the folder openaev doesn't exist
$installationDir=[System.Uri]::EscapeDataString("$OpenAEVPath")
$OpenAEVService = "${OPENAEV_SERVICE_NAME}" -replace "openbas", "openaev"
$OpenAEVService = "$OpenAEVService" -replace "OBAS", "OAEV"
$serviceName=[System.Uri]::EscapeDataString("$OpenAEVService")
Invoke-WebRequest -Uri "${OPENAEV_URL}/api/agent/installer/openaev/windows/session-user/${OPENAEV_TOKEN}?installationDir=$installationDir"&"serviceName=$serviceName" -OutFile "openaev-installer.ps1";
./openaev-installer.ps1
$AgentPath = $AgentPath -replace "openaev", "openbas"
$AgentPath = $AgentPath -replace "OAEV", "OBAS"
Get-Process | Where-Object { $_.Path -eq "$AgentPath" } | Stop-Process -Force;
$UninstallDir = "${OPENAEV_INSTALL_DIR}" -replace "openaev", "openbas"
$UninstallDir = "${OPENAEV_INSTALL_DIR}" -replace "OAEV", "OBAS"
rm -Force "${UninstallDir}/openbas.ico"
rm -Force "${UninstallDir}/openbas_agent_kill.ps1"
rm -Force "${UninstallDir}/openbas_agent_start.ps1"
rm -Force "${UninstallDir}/openbas-agent.exe"
rm -Force "${UninstallDir}/openbas-agent-config.toml"
rm -Force "${UninstallDir}/uninstall.exe"
if ($isElevated) {
    schtasks.exe /End /TN "$AgentName"
    schtasks.exe /Delete /TN "$AgentName" /F
} else {
    Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "$AgentName"
}
rm -force ./openaev-installer.ps1
}
rm -force ./openaev-installer-session-user.exe;