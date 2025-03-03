#!/bin/sh
set -e

# --- Parse command-line arguments ---
USER_ARG=""
GROUP_ARG=""

while [ $# -gt 0 ]; do
  case "$1" in
    --user)
      shift
      USER_ARG="$1"
      ;;
    --group)
      shift
      GROUP_ARG="$1"
      ;;
    *)
      echo "Usage: $0 --user [user] --group [group]"
      exit 1
      ;;
  esac
  shift
done

# --- Validate that user and group are provided ---
if [ -z "$USER_ARG" ]; then
  echo "Error: --user argument is required and cannot be empty."
  exit 1
fi

if [ -z "$GROUP_ARG" ]; then
  echo "Error: --group argument is required and cannot be empty. You can find your groups with the command 'id'."
  exit 1
fi

# --- Verify that the user exists ---
if ! id "$USER_ARG" >/dev/null 2>&1; then
  echo "Error: User '$USER_ARG' does not exist."
  exit 1
fi

# --- Verify that the group exists ---
if ! dscl . read /Groups/"$GROUP_ARG" >/dev/null 2>&1; then
  echo "Error: Group '$GROUP_ARG' does not exist. You can find your groups with the command 'id'."
  exit 1
fi

base_url=${OPENBAS_URL}
architecture=$(uname -m)
user="$USER_ARG"
group="$GROUP_ARG"

install_dir="/opt/openbas-agent-service-${user}"
service_name="${user}-openbas-agent"

os=$(uname | tr '[:upper:]' '[:lower:]')
if [ "${os}" = "darwin" ]; then
  os="macos"
fi

if [ "${os}" != "macos" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openbas github project"
  exit 1
fi

echo "Starting install script for ${os} | ${architecture}"

echo "01. Stopping existing ${service_name}..."
launchctl bootout system/ ~/Library/LaunchDaemons/${service_name}.plist || echo "${service_name} already stopped"

echo "02. Downloading OpenBAS Agent into ${install_dir}..."
(mkdir -p ${install_dir} && touch ${install_dir} >/dev/null 2>&1) || (echo -n "\nFatal: Can't write to /opt\n" >&2 && exit 1)
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
mkdir -p ~/Library/LaunchDaemons
cat > ~/Library/LaunchDaemons/${service_name}.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
    <dict>
        <key>Label</key>
        <string>${service_name}</string>

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

        <key>UserName</key>
        <string>${user}</string>
        <key>GroupName</key>
        <string>${group}</string>
        <key>InitGroups</key>
        <true/>
    </dict>
</plist>
EOF

chown -R ${user}:${group} ${install_dir}
echo "05. Starting agent service"
launchctl enable system/${service_name}
launchctl bootstrap system/ ~/Library/LaunchDaemons/${service_name}.plist

echo "OpenBAS Agent Service User started."