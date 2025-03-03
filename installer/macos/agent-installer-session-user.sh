#!/bin/sh
set -e

base_url=${OPENBAS_URL}
architecture=$(uname -m)

install_dir="/Users/$(id -un)/.local/openbas-agent-session"
session_name="openbas-agent-session"

os=$(uname | tr '[:upper:]' '[:lower:]')
if [ "${os}" = "darwin" ]; then
  os="macos"
fi

if [ "${os}" != "macos" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openbas github project"
  exit 1
fi

echo "Starting install script for ${os} | ${architecture}"

echo "01. Stopping existing ${session_name}..."
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/${session_name}.plist || echo "${session_name} already stopped"

echo "02. Downloading OpenBAS Agent into ${install_dir}..."
(mkdir -p ${install_dir} && touch ${install_dir} >/dev/null 2>&1) || (echo -n "\nFatal: Can't write to $HOME/.local\n" >&2 && exit 1)
curl -sSfL ${base_url}/api/agent/executable/openbas/${os}/${architecture} -o ${install_dir}/openbas-agent
chmod +x ${install_dir}/openbas-agent

echo "03. Creating OpenBAS configuration file"
cat > ${install_dir}/openbas-agent-config.toml <<EOF
debug=false

[openbas]
url = "${OPENBAS_URL}"
token = "${OPENBAS_TOKEN}"
unsecured_certificate = "${OPENBAS_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENBAS_WITH_PROXY}"
EOF

echo "04. Writing agent service"
mkdir -p ~/Library/LaunchAgents
cat > ~/Library/LaunchAgents/${session_name}.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
    <dict>
        <key>Label</key>
        <string>${session_name}</string>

        <key>Program</key>
        <string>${install_dir}/openbas-agent</string>

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
launchctl enable user/$(id -u)/~/Library/LaunchAgents/${session_name}.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/${session_name}.plist

echo "OpenBAS Agent Session User started."