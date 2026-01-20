# What Should I Monitor with Panoptes?

This guide helps you identify which files and paths to monitor in your Kubernetes environment for effective security and compliance.

## Quick Reference: Critical Paths

| Path | Why Monitor | Argus (FIM) | Janus (Access) |
|------|-------------|:-----------:|:--------------:|
| `/etc/passwd`, `/etc/shadow` | User account changes | Yes | Yes (deny) |
| `/etc/sudoers`, `/etc/sudoers.d/` | Privilege escalation | Yes | Yes (deny) |
| `/var/log/` | Log tampering detection | Yes | - |
| `/etc/ssh/` | SSH configuration changes | Yes | - |
| `/root/.ssh/` | Root SSH key modifications | Yes | Yes (deny) |
| `/var/run/secrets/kubernetes.io` | K8s service account tokens | - | Yes (audit) |
| `/etc/pam.d/` | Authentication config | Yes | - |
| `/etc/cron.d/`, `/etc/crontab` | Scheduled task persistence | Yes | - |

## Understanding Argus vs Janus

### Argus (File Integrity Monitoring)
- **Purpose**: Detect when files are created, modified, or deleted
- **Use for**: Configuration files, logs, binaries, certificates
- **Events**: `create`, `modify`, `delete`, `attrib`, `moved_from`, `moved_to`
- **Best for**: "What changed?" questions

### Janus (File Access Control)
- **Purpose**: Monitor and control who accesses files
- **Use for**: Credentials, secrets, sensitive data
- **Events**: `access`, `open`, `execute`
- **Best for**: "Who accessed this?" and "Prevent unauthorized access" questions

## Monitoring by Workload Type

### Web Servers (nginx, Apache, Caddy)

```yaml
# ArgusWatcher subjects
subjects:
  # Configuration files
  - paths:
      - /etc/nginx
      - /etc/apache2
      - /etc/caddy
    events: [modify, create, delete]
    recursive: true

  # SSL certificates
  - paths:
      - /etc/ssl/certs
      - /etc/letsencrypt
    events: [modify, create, delete, attrib]
    recursive: true
    tags:
      severity: critical

  # Web content (optional - high volume)
  - paths:
      - /var/www
      - /usr/share/nginx/html
    events: [modify, create, delete]
    recursive: true
    maxDepth: 3
    ignore:
      - "*.log"
      - "*.tmp"
```

### Databases (PostgreSQL, MySQL, MongoDB)

```yaml
# ArgusWatcher subjects
subjects:
  # Configuration files
  - paths:
      - /etc/postgresql
      - /etc/mysql
      - /etc/mongod.conf
    events: [modify, create, delete]
    recursive: true
    tags:
      severity: critical

  # Authentication files
  - paths:
      - /var/lib/postgresql/*/pg_hba.conf
      - /etc/mysql/debian.cnf
    events: [modify]
    tags:
      severity: critical

# JanusGuard - audit data directory access
subjects:
  - allow:
      - /var/lib/postgresql
      - /var/lib/mysql
    events: [access, open]
    audit: true
```

### API Services / Microservices

```yaml
# ArgusWatcher subjects
subjects:
  # Application configuration
  - paths:
      - /app/config
      - /etc/app
      - /opt/app/config
    events: [modify, create, delete]
    recursive: true

  # Environment files (often contain secrets)
  - paths:
      - /app/.env
      - /app/.env.local
    events: [modify, create, delete]
    tags:
      severity: critical

# JanusGuard - protect secrets
subjects:
  - deny:
      - /app/.env
      - /app/secrets
      - /var/run/secrets
    events: [access, open]
    autoAllowOwner: true
    audit: true
```

### Message Queues (Kafka, RabbitMQ, Redis)

```yaml
# ArgusWatcher subjects
subjects:
  - paths:
      - /etc/kafka
      - /etc/rabbitmq
      - /etc/redis
    events: [modify, create, delete]
    recursive: true

  # Data directories (optional - may be high volume)
  - paths:
      - /var/lib/kafka
      - /var/lib/rabbitmq
    events: [delete]  # Only alert on deletions
    recursive: true
    maxDepth: 2
```

### CI/CD Runners (Jenkins, GitLab Runner)

```yaml
# ArgusWatcher - high-risk environment
subjects:
  # Runner configuration
  - paths:
      - /etc/gitlab-runner
      - /var/jenkins_home
    events: [modify, create, delete]
    recursive: true

  # Build artifacts that might contain secrets
  - paths:
      - /builds
      - /workspace
    events: [create]
    recursive: true
    maxDepth: 3
    ignore:
      - "*.o"
      - "*.class"
      - "node_modules/*"

# JanusGuard - prevent credential theft
subjects:
  - deny:
      - /var/run/docker.sock
      - /root/.docker/config.json
    events: [access, open]
    audit: true
    tags:
      severity: critical
```

## Monitoring by Security Concern

### Credential Theft Prevention

```yaml
# JanusGuard
subjects:
  - deny:
      - /etc/shadow
      - /etc/gshadow
      - /root/.ssh/id_*
      - /home/*/.ssh/id_*
      - /root/.aws/credentials
      - /root/.kube/config
    events: [access, open]
    defaultResponse: deny
    audit: true
```

### Persistence Detection

```yaml
# ArgusWatcher - common persistence mechanisms
subjects:
  # Cron jobs
  - paths:
      - /etc/cron.d
      - /etc/crontab
      - /var/spool/cron
      - /etc/cron.hourly
      - /etc/cron.daily
    events: [modify, create, delete]
    recursive: true

  # Systemd services
  - paths:
      - /etc/systemd/system
      - /lib/systemd/system
    events: [modify, create, delete]
    recursive: true

  # Init scripts
  - paths:
      - /etc/init.d
      - /etc/rc.local
    events: [modify, create, delete]

  # User profile scripts
  - paths:
      - /etc/profile.d
      - /etc/bash.bashrc
      - /root/.bashrc
      - /root/.profile
    events: [modify, create]
```

### Malware/Rootkit Detection

```yaml
# ArgusWatcher - binary modifications
subjects:
  - paths:
      - /usr/bin
      - /usr/sbin
      - /bin
      - /sbin
      - /usr/local/bin
    events: [modify, create]
    tags:
      severity: critical
      category: binary-modification

  # Library modifications
  - paths:
      - /lib
      - /lib64
      - /usr/lib
    events: [modify, create]
    recursive: true
    maxDepth: 2
```

### Data Exfiltration Prevention

```yaml
# JanusGuard - audit data access
subjects:
  # Audit bulk data access
  - allow:
      - /data
      - /var/lib/app/data
      - /export
    events: [access, open]
    audit: true

  # Block common exfil tools
  - deny:
      - /usr/bin/curl
      - /usr/bin/wget
      - /usr/bin/scp
      - /usr/bin/rsync
    events: [execute]
    defaultResponse: audit  # or deny
    audit: true
```

## Event Selection Guide

### Argus Events

| Event | Use Case |
|-------|----------|
| `create` | New file creation (persistence, malware) |
| `modify` | Content changes (config tampering) |
| `delete` | File removal (log tampering, cleanup) |
| `attrib` | Permission changes (privilege escalation) |
| `moved_from`/`moved_to` | File relocation (hiding) |
| `all` | Full monitoring (high-security paths) |

### Janus Events

| Event | Use Case |
|-------|----------|
| `access` | File metadata access |
| `open` | File content reading |
| `execute` | Binary execution |
| `all` | Complete access auditing |

## Performance Considerations

### High-Volume Paths (Use Carefully)

- `/var/log` - Use `ignore` patterns for rotated logs
- `/tmp`, `/var/tmp` - Often too noisy
- Application data directories - Use `maxDepth` limits

### Recommended Patterns

```yaml
# Good: Specific with ignores
- paths: [/var/log]
  events: [delete, modify]
  ignore:
    - "*.gz"
    - "*.old"
    - "*.[0-9]"

# Avoid: Too broad
- paths: [/]
  events: [all]
  recursive: true
```

### Using maxDepth

```yaml
# Limit recursive depth
- paths: [/etc]
  events: [modify, create, delete]
  recursive: true
  maxDepth: 2  # Only /etc/* and /etc/*/*
```

## Next Steps

1. **Start with base-security.yaml**: Apply the [base security template](../compliance-templates/base-security.yaml)
2. **Add compliance templates**: Apply framework-specific templates as needed
3. **Review events in dashboard**: Monitor the Panoptes Eye Events page
4. **Tune ignores**: Add ignore patterns for noisy paths
5. **Enable enforcement**: After validating, set `enforcing: true` on JanusGuards

## Related Documentation

- [Compliance Templates](../compliance-templates/README.md)
- [Monitoring by Compliance Framework](./monitoring-by-compliance.md)
- [Quick Start Security Guide](./quick-start-security.md)
