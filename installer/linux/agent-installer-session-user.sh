#!/bin/sh
set -e

base_url=${OPENAEV_URL}
architecture=$(uname -m)
systemd_status=$(systemctl is-system-running)

os=$(uname | tr '[:upper:]' '[:lower:]')
install_dir="$HOME/${OPENAEV_INSTALL_DIR}"
session_name="${OPENAEV_SERVICE_NAME}"
systemd_unit_dir="$HOME/.config/systemd/user/"

if [ "${os}" != "linux" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openaev github project"
  exit 1
fi


if [ "$systemd_status" != "running" ] && [ "$systemd_status" != "degraded" ]; then
  echo "Systemd is in unexpected state: $systemd_status. Installation is not supported."
  exit 1
else
  echo "Systemd is in acceptable state: $systemd_status"
fi

echo "Starting install script for ${os} | ${architecture}"

echo "01. Stopping existing ${session_name}..."
systemctl --user stop ${session_name} || echo "Fail stopping ${session_name}"

echo "02. Downloading OpenAEV Agent into ${install_dir}..."
(mkdir -p ${install_dir} && touch ${install_dir} >/dev/null 2>&1) || (echo -n "\nFatal: Can't write to ${install_dir}\n" >&2 && exit 1)
curl -sSfL ${base_url}/api/agent/executable/openaev/${os}/${architecture} -o ${install_dir}/openaev-agent
chmod +x ${install_dir}/openaev-agent

echo "03. Creating OpenAEV configuration file"
cat > ${install_dir}/openaev-agent-config.toml <<EOF
debug=false

[openaev]
url = "${OPENAEV_URL}"
token = "${OPENAEV_TOKEN}"
unsecured_certificate = "${OPENAEV_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENAEV_WITH_PROXY}"
installation_mode = "session-user"
service_name = "${OPENAEV_SERVICE_NAME}"
EOF

echo "04. Writing agent service"
cat > ${install_dir}/${session_name}.service <<EOF
[Unit]
Description=OpenAEV Agent Session
After=network.target
[Service]
Type=exec
ExecStart=${install_dir}/openaev-agent
StandardOutput=journal
[Install]
WantedBy=default.target
EOF

echo "05. Starting agent service"
(
  mkdir -p $systemd_unit_dir
  ln -sf ${install_dir}/${session_name}.service $systemd_unit_dir
  systemctl --user daemon-reload
  systemctl --user enable ${session_name}
  systemctl --user start ${session_name}
) || (echo "Error while enabling OpenAEV Agent systemd unit file or starting the agent" >&2 && exit 1)

echo "OpenAEV Agent started."
