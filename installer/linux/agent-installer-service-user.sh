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
if ! getent group "$GROUP_ARG" >/dev/null 2>&1; then
  echo "Error: Group '$GROUP_ARG' does not exist. You can find your groups with the command 'id'."
  exit 1
fi

base_url=${OPENBAS_URL}
architecture=$(uname -m)
user="$USER_ARG"
group="$GROUP_ARG"

os=$(uname | tr '[:upper:]' '[:lower:]')
systemd_status=$(systemctl is-system-running)
install_dir="$HOME/${OPENBAS_INSTALL_DIR}-${user}"
service_name="${user}-${OPENBAS_SERVICE_NAME}"


if [ "${os}" != "linux" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openbas github project"
  exit 1
fi

if [ "$systemd_status" != "running" ] && [ "$systemd_status" != "degraded" ]; then
  echo "Systemd is in unexpected state: $systemd_status. Installation is not supported."
  exit 1
else
  echo "Systemd is in acceptable state: $systemd_status"
fi

echo "Starting install script for ${os} | ${architecture}"

echo "01. Stopping existing ${service_name}..."
systemctl stop ${service_name} || echo "Fail stopping ${service_name}"

echo "02. Downloading OpenBAS Agent into ${install_dir}..."
(mkdir -p ${install_dir} && touch ${install_dir} >/dev/null 2>&1) || (echo -n "\nFatal: Can't write to ${install_dir}\n" >&2 && exit 1)
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
installation_mode = "service-user"
service_name = "${OPENBAS_SERVICE_NAME}"
EOF

echo "04. Writing agent service"
cat > ${install_dir}/${service_name}.service <<EOF
[Unit]
Description=OpenBAS Agent Service ${user}
After=network.target
[Service]
User=${user}
Group=${group}
Type=exec
ExecStart=${install_dir}/openbas-agent
StandardOutput=journal
Restart=always
RestartSec=60
[Install]
WantedBy=multi-user.target
EOF

chown -R ${user}:${group} ${install_dir}
echo "05. Starting agent service"
(
  ln -sf ${install_dir}/${service_name}.service /etc/systemd/system/
  systemctl daemon-reload
  systemctl enable ${service_name}
  systemctl start ${service_name}
) || (echo "Error while enabling OpenBAS Agent systemd unit file or starting the agent" >&2 && exit 1)

echo "OpenBAS Agent started."
