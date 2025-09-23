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
if ([string]::IsNullOrEmpty($architecture)) { throw "Architecture $env:PROCESSOR_ARCHITECTURE is not supported yet, please create a ticket in openbas github project" }
function Sanitize-UserName {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserName
    )
    $UserName = $UserName.ToLower()
    $pattern = '[\/\\:\*\?<>\|]'
    return ($UserName -replace $pattern, '')
}
$BasePath = "${OPENBAS_INSTALL_DIR}";
$User = whoami;
$SanitizedUser =  Sanitize-UserName -UserName $user;
$isElevated = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($isElevated) {
    $AgentName = "${OPENBAS_SERVICE_NAME}-Administrator-$SanitizedUser"
} else {
    $AgentName = "${OPENBAS_SERVICE_NAME}-$SanitizedUser"
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

$AgentPath = $InstallDir + "\openbas-agent.exe";

Get-Process | Where-Object { $_.Path -eq "$AgentPath" } | Stop-Process -Force;
Invoke-WebRequest -Uri "${OPENBAS_URL}/api/agent/package/openbas/windows/${architecture}/session-user" -OutFile "openbas-installer-session-user.exe";

./openbas-installer-session-user.exe /S ~OPENBAS_URL="${OPENBAS_URL}" ~ACCESS_TOKEN="${OPENBAS_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENBAS_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENBAS_WITH_PROXY} ~SERVICE_NAME="${OPENBAS_SERVICE_NAME}" ~INSTALL_DIR="$CleanBasePath";

rm -force ./openbas-installer-session-user.exe;