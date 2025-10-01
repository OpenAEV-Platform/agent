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

function Sanitize-UserName {
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
$SanitizedUser = Sanitize-UserName -UserName $user;
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

Invoke-WebRequest -Uri "${OPENAEV_URL}/api/agent/executable/openaev/windows/${architecture}" -OutFile $AgentUpgradedPath;

sc.exe stop $AgentName;

rm -force $AgentPath;
mv $AgentUpgradedPath $AgentPath;

sc.exe start $AgentName;