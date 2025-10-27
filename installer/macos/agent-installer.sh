#!/bin/sh
set -e

base_url=${OPENAEV_URL}
architecture=$(uname -m)

install_dir="${OPENAEV_INSTALL_DIR}"
service_name="${OPENAEV_SERVICE_NAME}"

os=$(uname | tr '[:upper:]' '[:lower:]')
if [ "${os}" = "darwin" ]; then
  os="macos"
fi

if [ "${os}" != "macos" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openaev github project"
  exit 1
fi

echo "Starting install script for ${os} | ${architecture}"

echo "01. Stopping existing ${service_name}..."
launchctl bootout system /Library/LaunchDaemons/io.filigran.${service_name}.plist || echo "io.filigran.${service_name} already stopped"

echo "02. Downloading OpenAEV Agent into ${install_dir}..."
(mkdir -p ${install_dir} && touch ${install_dir} >/dev/null 2>&1) || (echo -n "\nFatal: Can't write to ${install_dir}\n" >&2 && exit 1)
curl -sSfL ${base_url}/api/agent/executable/openaev/${os}/${architecture} -o ${install_dir}/openaev-agent
chmod 755 ${install_dir}/openaev-agent

echo "03. Creating OpenAEV configuration file"
cat > ${install_dir}/openaev-agent-config.toml <<EOF
debug=false

[openaev]
url = "${OPENAEV_URL}"
token = "${OPENAEV_TOKEN}"
unsecured_certificate = "${OPENAEV_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENAEV_WITH_PROXY}"
installation_mode = "service"
service_name = "${OPENAEV_SERVICE_NAME}"
EOF

echo "04. Writing agent service"
mkdir -p /Library/LaunchDaemons
cat > /Library/LaunchDaemons/io.filigran.${service_name}.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
    <dict>
        <key>Label</key>
        <string>io.filigran.${service_name}</string>

        <key>Program</key>
        <string>${install_dir}/openaev-agent</string>

        <key>RunAtLoad</key>
        <true/>

        <!-- The agent needs to run at all times -->
        <key>KeepAlive</key>
        <true/>

        <!-- This prevents macOS from limiting the resource usage of the agent -->
        <key>ProcessType</key>
        <string>Interactive</string>

        <!-- Increase the frequency of restarting the agent on failure, or post-update -->
        <key>ThrottleInterval</key>
        <integer>60</integer>

        <!-- Wait for 10 minutes for the agent to shut down (the agent itself waits for tasks to complete) -->
        <key>ExitTimeOut</key>
        <integer>600</integer>

        <key>StandardOutPath</key>
        <string>${install_dir}/runner.log</string>
        <key>StandardErrorPath</key>
        <string>${install_dir}/runner.log</string>
    </dict>
</plist>
EOF

echo "05. Starting agent service"
launchctl enable system/io.filigran.${service_name}
launchctl bootstrap system /Library/LaunchDaemons/io.filigran.${service_name}.plist

echo "OpenAEV Agent started."