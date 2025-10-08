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

# Manage the renaming OpenBAS -> OpenAEV ...
$OpenAEVPath = "${OPENAEV_INSTALL_DIR}" -replace "openbas", "openaev"
$OpenAEVPath = "$OpenAEVPath" -replace "OBAS", "OAEV"
if(Test-Path "$OpenAEVPath")
{
# Upgrade the agent if the folder *OAEV* exists
Invoke-WebRequest -Uri "${OPENAEV_URL}/api/agent/package/openaev/windows/${architecture}/service" -OutFile "openaev-installer.exe"; ./openaev-installer.exe /S ~OPENAEV_URL="${OPENAEV_URL}" ~ACCESS_TOKEN="${OPENAEV_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENAEV_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENAEV_WITH_PROXY} ~SERVICE_NAME="${OPENAEV_SERVICE_NAME}" ~INSTALL_DIR="${OPENAEV_INSTALL_DIR}" | Out-Null;
}
else
{
# Uninstall the old named agent *OBAS* and install the new named agent *OAEV* if the folder OAEV doesn't exist
$installationDir=[System.Uri]::EscapeDataString("$OpenAEVPath")
$OpenAEVService = "${OPENAEV_SERVICE_NAME}" -replace "openbas", "openaev"
$OpenAEVService = "$OpenAEVService" -replace "OBAS", "OAEV"
$serviceName=[System.Uri]::EscapeDataString("$OpenAEVService")
Invoke-WebRequest -Uri "${OPENAEV_URL}/api/agent/installer/openaev/windows/service/${OPENAEV_TOKEN}?installationDir=$installationDir"&"serviceName=$serviceName" -OutFile "openaev-installer.ps1";
./openaev-installer.ps1
sc.exe stop "${OPENAEV_SERVICE_NAME}"
$UninstallDir = "${OPENAEV_INSTALL_DIR}" -replace "openaev", "openbas"
$UninstallDir = "${OPENAEV_INSTALL_DIR}" -replace "OAEV", "OBAS"
rm -force "${UninstallDir}/openbas.ico"
rm -force "${UninstallDir}/openbas_agent_kill.ps1"
rm -force "${UninstallDir}/openbas-agent.exe"
rm -force "${UninstallDir}/openbas-agent-config.toml"
rm -force "${UninstallDir}/uninstall.exe"
sc.exe delete "${OPENAEV_SERVICE_NAME}"
rm -force ./openaev-installer.ps1
}
rm -force ./openaev-installer.exe;