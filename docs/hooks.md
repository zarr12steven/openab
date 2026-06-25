# Lifecycle Hooks

OpenAB supports lifecycle hooks that run at specific points during the container lifecycle. All lifecycle phases are configured in `config.toml` under the `[hooks]` table.

## Lifecycle Order

```
hooks.pre_seed → hooks.pre_boot → (agent running) → hooks.pre_shutdown
```

| Phase | Purpose | Config | Action Type |
|-------|---------|--------|-------------|
| `pre_seed` | Download & extract S3 archives to seed the environment | `[hooks.pre_seed]` | Built-in S3 download + extract |
| `pre_boot` | Run custom setup scripts before agent pool creation | `[hooks.pre_boot]` | User script |
| `pre_shutdown` | Run custom cleanup scripts after pool shutdown | `[hooks.pre_shutdown]` | User script |

## Pre-Seed Phase

The `pre_seed` phase runs **before** `pre_boot`. It downloads archives from S3 and extracts them into the agent's home directory (or a custom target). Supported formats: `.zip`, `.tar.gz`, and `.tgz` (auto-detected via magic bytes). This eliminates the need for users to install AWS CLI and write download scripts in `pre_boot`.

> `pre-seed` is enabled by default. No feature flag needed.

### Configuration

```toml
[hooks.pre_seed]
sources = [
  "s3://my-bucket/base-env.tar.gz",
  "s3://my-bucket/shared-memory.zip",
  "s3://my-bucket/agent-overrides.tgz",
]
# target = "/home/agent"                  # default: $HOME
# max_bytes = 104857600                   # max compressed size per archive (default: 100 MiB)
# timeout_seconds = 300                   # per-source timeout (default: 300)
# on_failure = "abort"                    # "abort" or "warn" (default: "abort")
# region = "us-west-2"                    # optional: override AWS region
# endpoint_url = "http://localhost:4566"  # optional: LocalStack / VPC endpoint
```

### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `sources` | string[] | `[]` | S3 URIs of archives (`.zip`, `.tar.gz`, `.tgz`). Max 5. Extracted in order. |
| `target` | string | `$HOME` | Extraction target directory. |
| `max_bytes` | u64 | `104857600` | Max compressed archive size in bytes (100 MiB). |
| `timeout_seconds` | u64 | `300` | Per-source download+extract timeout. |
| `on_failure` | string | `"abort"` | `"abort"` exits openab; `"warn"` logs and continues. |
| `region` | string | — | Override AWS region. |
| `endpoint_url` | string | — | Override S3 endpoint URL. |

### Layer Concept

Sources are extracted sequentially (first → last). Files from later archives overwrite earlier ones — like layers in a container image:

```
Layer 3 (last)   ─── highest priority, overwrites all below
Layer 2          ─── overwrites layer 1
Layer 1 (first)  ─── base layer
─────────────────
     $HOME
```

### Safety

- **Integrity verification**: two layers of protection:
  1. **S3-native checksum (automatic)**: if the object was uploaded with `--checksum-algorithm SHA256`, OpenAB automatically verifies it on download — no config needed
  2. **User-provided `sha256s` (optional)**: explicit checksums in config for additional defense-in-depth
- **Size cap**: downloads exceeding `max_bytes` are rejected before extraction
- **Atomic extraction**: archives are first extracted to a temp directory, then moved into target — if extraction fails, target is not corrupted. Note: the move phase is per-file; if it fails mid-way with `on_failure = "warn"`, the target may be partially updated.
- **Path traversal prevention**: zip uses `enclosed_name()`; tarball uses `unpack_in()` which rejects `..` escapes
- **Permission hardening**: suid/sgid/sticky bits are stripped from extracted files

### Constraints

- Maximum **5** sources
- Only `s3://` URIs supported
- Supported formats: `.zip`, `.tar.gz`, `.tgz` (auto-detected via gzip magic bytes)
- Uses the standard AWS credential chain (IRSA, ECS task role, env vars)
- Optional `region`/`endpoint_url` override for LocalStack or VPC endpoints

### IAM Policy

```json
{
  "Effect": "Allow",
  "Action": ["s3:GetObject"],
  "Resource": [
    "arn:aws:s3:::my-bucket/base-env.zip",
    "arn:aws:s3:::my-bucket/shared-memory.zip",
    "arn:aws:s3:::my-bucket/agent-overrides.zip"
  ]
}
```

### Recommended: Enable S3 Checksums on Upload

For automatic integrity verification without maintaining `sha256s` in config, upload zip archives with SHA-256 checksums enabled:

```bash
# Upload with SHA-256 checksum (recommended)
aws s3 cp env.zip s3://my-bucket/env.zip --checksum-algorithm SHA256

# Verify it was stored
aws s3api head-object --bucket my-bucket --key env.zip --checksum-mode ENABLED
```

When objects have S3-native SHA-256 checksums, OpenAB verifies them automatically on download — no `sha256s` config needed. This is the simplest path to integrity verification.

> **Note:** If `sha256s` is also provided in config, both checks run. The S3-native check uses the base64-encoded checksum from the `x-amz-checksum-sha256` response header. If neither is available, download proceeds without integrity verification (relies on IAM + bucket policy for trust).

---

## Available Hooks

| Hook | Timing | Use Case |
|------|--------|----------|
| `pre_boot` | Before agent pool creation | Bootstrap files, sync from S3, install CLIs |
| `pre_shutdown` | After pool shutdown, before exit | Backup state, sync to S3 |

## Configuration

Each hook supports exactly **one** script source:

### Option A: File path

```toml
[hooks.pre_boot]
script = "/etc/openab/pre-boot.sh"
timeout_seconds = 60
on_failure = "abort"
```

### Option B: Inline script

```toml
[hooks.pre_boot]
inline = '''
#!/bin/sh
set -e
aws s3 sync "$BOOTSTRAP_URI" "$HOME/"
'''
timeout_seconds = 120
on_failure = "abort"
```

### Option C: Remote URL (with SHA-256 verification)

```toml
[hooks.pre_boot]
url = "https://raw.githubusercontent.com/acme/config/main/pre-boot.sh"
sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
timeout_seconds = 60
on_failure = "abort"
```

## Fields

| Field | Default | Description |
|-------|---------|-------------|
| `script` | — | Absolute path to an executable script |
| `inline` | — | Script content (written to temp file and executed) |
| `url` | — | Remote script URL (max 1 MiB) |
| `sha256` | — | Required with `url` — hex-encoded SHA-256 of the script |
| `timeout_seconds` | `60` | Max wall-clock time before the script is killed |
| `on_failure` | `"abort"` | `"abort"` exits openab; `"warn"` logs and continues |

## Validation Rules

- Exactly one of `script`, `inline`, or `url` must be set
- `url` requires `sha256`
- `script` must be an absolute path

Validation runs at startup — config errors are caught before any side effects.

## Environment

Scripts run with a sanitized environment:

**Always passed:**
- `HOME`, `PATH`, `USER` (unix) / `USERPROFILE`, `USERNAME`, `SystemRoot`, `SystemDrive` (windows)

**Cloud credentials (auto-detected and passed through):**
- `AWS_*`, `AMAZON_*`, `ECS_CONTAINER_METADATA_URI*`
- `GOOGLE_*`, `GCLOUD_*`, `CLOUDSDK_*`
- `AZURE_*`

**Bootstrap variables (passed if set):**
- `BOOTSTRAP_URI`, `BOOTSTRAP_BASE_URI`, `BOOTSTRAP_PERSONAL_URI`
- `STATE_BUCKET`, `TASK_FAMILY`

**OpenAB identity (passed if set):**
- `OPENAB_AGENT_NAME` — the agent's configured name
- `OPENAB_BACKEND_AGENT` — the backend agent type (e.g. `claude`, `codex`)

> **Note:** `DISCORD_BOT_TOKEN` and other openab secrets are NOT exposed to hook scripts.

## Security

- Temp files are created atomically with `0700` permissions (unix)
- Remote scripts require SHA-256 verification — openab refuses to execute on mismatch
- Scripts run as the container's UID (not root, unless the container runs as root)
- Remote script size is capped at 1 MiB

## Examples

### Sync config from S3 on startup

```toml
[hooks.pre_boot]
timeout_seconds = 120
on_failure = "abort"
inline = '''
#!/bin/sh
set -e
if [ ! -f "$HOME/AGENTS.md" ]; then
  aws s3 sync "$BOOTSTRAP_BASE_URI" "$HOME/"
fi
'''
```

### Backup state on shutdown (ECS Fargate)

```toml
[hooks.pre_shutdown]
timeout_seconds = 30
on_failure = "warn"
inline = '''
#!/bin/sh
aws s3 sync "$HOME/" "s3://$STATE_BUCKET/$TASK_FAMILY/" \
  --exclude "aws-cli/*" --exclude "bin/*" --quiet
'''
```

### Conditional OAuth token refresh

```toml
[hooks.pre_boot]
script = "/etc/openab/pre-boot.sh"
timeout_seconds = 60
on_failure = "warn"
```

```bash
#!/bin/sh
# /etc/openab/pre-boot.sh
set -e
if [ -f "$HOME/.kiro/auth.json" ]; then
  EXPIRES=$(jq -r '.expires' "$HOME/.kiro/auth.json")
  NOW=$(date +%s)
  if [ "$NOW" -gt "$EXPIRES" ]; then
    kiro-cli auth refresh || true
  fi
fi
```

## Platform Comparison

| Option | Best for | Requires redeploy? | Network at boot? |
|--------|----------|--------------------|-------------------|
| `script` | k8s (ConfigMap mount), EFS, image bake | Only if image-baked | No |
| `inline` | ECS, Docker Compose, bare metal | Config change only | No |
| `url` + `sha256` | Central script repo, multi-cluster | No (update sha256 to roll) | Yes |
