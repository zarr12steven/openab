# OpenShell

Run OAB inside an [NVIDIA OpenShell](https://github.com/NVIDIA/OpenShell) sandbox for isolated, policy-enforced execution with credential injection.

## Prerequisites

- Docker running on the host
- [OpenShell CLI](https://github.com/NVIDIA/OpenShell#install) installed

```bash
curl -LsSf https://raw.githubusercontent.com/NVIDIA/OpenShell/main/install.sh | sh
```

## Quick Start (Local Docker)

The following is a single copy-pasteable sequence. All commands run **on the host** unless prefixed with `sandbox$`.

```bash
# 1. Create credential providers
#    Providers are stored in the OpenShell gateway's local state.
#    Host env vars are read only at creation time and not retained.
#    Providers persist until explicitly removed with `openshell provider delete <name>`.
export DISCORD_BOT_TOKEN="your-token"
export GITHUB_TOKEN="your-token"
export ANTHROPIC_API_KEY="your-key"

openshell provider create --name discord --env DISCORD_BOT_TOKEN
openshell provider create --name github --env GITHUB_TOKEN
openshell provider create --name anthropic --env ANTHROPIC_API_KEY

# 2. Create sandbox with providers and port forwarding
#    This starts an isolated container and drops you into a bash shell inside it.
#    The sandbox runs until you `exit` or delete it from the host.
openshell sandbox create --name oab \
  --provider discord \
  --provider github \
  --provider anthropic \
  --forward 3000 \
  -- bash
```

At this point you are **inside the sandbox** (prompt changes). To return to the host, type `exit`. To reconnect later: `openshell sandbox connect oab`.

```bash
# 3. (Inside sandbox) Download and install OAB
sandbox$ TAG=$(curl -sI https://github.com/openabdev/openab/releases/latest | grep -i location | sed 's|.*/||' | tr -d '\r')
sandbox$ curl -LO "https://github.com/openabdev/openab/releases/download/${TAG}/${TAG}-linux-x64.tar.gz"
sandbox$ tar xzf ${TAG}-linux-x64.tar.gz
sandbox$ chmod +x openab

# 4. (Inside sandbox) Create config.toml
sandbox$ curl -LO https://raw.githubusercontent.com/openabdev/openab/main/config.toml.example
sandbox$ cp config.toml.example config.toml
sandbox$ sed -i 's/allowed_channels = \["1234567890"\]/allowed_channels = ["YOUR_CHANNEL_ID"]/' config.toml
```

Edit `config.toml` to set your Discord channel ID. The env vars (`DISCORD_BOT_TOKEN`, etc.) are already injected by the provider — no need to set them manually.

```bash
# 5. (Inside sandbox) Run OAB
sandbox$ ./openab serve --config config.toml
```

### Applying network policy (from a separate host terminal)

Open a new terminal on the host while OAB is running:

```bash
# All unlisted egress is denied by default.
cat > /tmp/oab-policy.yaml <<'EOF'
network:
  egress:
    - destination: "discord.com"
      ports: [443]
    - destination: "gateway.discord.gg"
      ports: [443]
    - destination: "api.github.com"
      ports: [443]
    - destination: "github.com"
      ports: [443]
    - destination: "api.anthropic.com"
      ports: [443]
EOF
openshell policy set oab --policy /tmp/oab-policy.yaml --wait
```

> **DNS note:** OpenShell resolves hostnames in `destination` via the sandbox's DNS at policy evaluation time. Wildcard subdomains (e.g., `*.discord.com`) are not supported — list each hostname explicitly. If a service uses multiple domains, check its docs for the full list.

## Credential Management

| Operation | Command |
|-----------|---------|
| List providers | `openshell provider list` |
| Delete a provider | `openshell provider delete discord` |
| Rotate a credential | Delete + recreate with new value |

Credentials are injected as env vars at sandbox runtime. They are **not** written to the sandbox filesystem. Removing a provider immediately revokes access on the next sandbox restart.

## Port Forwarding

Add `--forward <port>` at sandbox creation. Multiple ports are supported:

```bash
openshell sandbox create --name oab \
  --provider discord \
  --forward 3000 \
  --forward 8080 \
  -- bash
```

Each forwarded port creates an SSH tunnel: `localhost:<port>` on the host → `127.0.0.1:<port>` inside the sandbox. Tunnels are torn down when the sandbox is deleted.

## BYOC (Custom Image)

Build a custom sandbox image with OAB pre-installed:

```dockerfile
FROM ubuntu:24.04

RUN groupadd -g 1000660000 sandbox && \
    useradd -u 1000660000 -g sandbox -m sandbox

RUN apt-get update && apt-get install -y \
    curl git iproute2 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Download pre-built OAB binary
ARG OAB_VERSION=openab-0.8.4-beta.10
RUN curl -L "https://github.com/openabdev/openab/releases/download/${OAB_VERSION}/${OAB_VERSION}-linux-x64.tar.gz" | \
    tar xz -C /usr/local/bin/

USER sandbox
WORKDIR /home/sandbox
RUN curl -LO https://raw.githubusercontent.com/openabdev/openab/main/config.toml.example && \
    cp config.toml.example config.toml
```

Run it:

```bash
openshell sandbox create --name oab \
  --from ./Dockerfile \
  --provider discord \
  --provider github \
  --provider anthropic \
  --forward 3000 \
  -- bash

openshell policy set oab --policy /tmp/oab-policy.yaml --wait
openshell sandbox connect oab
```

Inside the sandbox, OAB is already installed:

```bash
sandbox$ sed -i 's/allowed_channels = \["1234567890"\]/allowed_channels = ["YOUR_CHANNEL_ID"]/' config.toml
sandbox$ openab serve --config config.toml
```

## Cleanup

```bash
openshell sandbox delete oab
# Optionally remove providers
openshell provider delete discord
openshell provider delete github
openshell provider delete anthropic
```
