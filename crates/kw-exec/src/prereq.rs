//! Local tool prerequisite checks (tofu, ansible, kubectl, helm, git, ssh).

use std::process::{Command, Stdio};

use crate::host_command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    pub name: &'static str,
    pub program: &'static str,
    pub version_args: &'static [&'static str],
    /// Minimum (major, minor).
    pub min: (u32, u32),
    /// Needed only for cloud provisioning.
    pub cloud_only: bool,
    pub install: InstallHint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallHint {
    pub macos: &'static str,
    pub linux: &'static str,
    pub windows: &'static str,
}

impl InstallHint {
    #[inline]
    pub const fn current(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            self.macos
        } else if cfg!(windows) {
            self.windows
        } else {
            self.linux
        }
    }
}

pub const TOOLS: [Tool; 6] = [
    Tool {
        name: "Ansible",
        program: "ansible-playbook",
        version_args: &["--version"],
        min: (2, 16),
        cloud_only: false,
        install: InstallHint {
            macos: "brew install ansible",
            linux: "pipx install --include-deps ansible  (or: sudo apt install ansible)",
            windows: "wsl --install, then inside WSL: sudo apt install ansible",
        },
    },
    Tool {
        name: "OpenTofu",
        program: "tofu",
        version_args: &["version"],
        min: (1, 8),
        cloud_only: true,
        install: InstallHint {
            macos: "brew install opentofu",
            linux: "curl -fsSL https://get.opentofu.org/install-opentofu.sh | sh -s -- --install-method standalone",
            windows: "winget install OpenTofu.Tofu",
        },
    },
    Tool {
        name: "kubectl",
        program: "kubectl",
        version_args: &["version", "--client"],
        min: (1, 30),
        cloud_only: false,
        install: InstallHint {
            macos: "brew install kubectl",
            linux: "sudo snap install kubectl --classic",
            windows: "winget install Kubernetes.kubectl",
        },
    },
    Tool {
        name: "Helm",
        program: "helm",
        version_args: &["version", "--short"],
        min: (3, 14),
        cloud_only: false,
        install: InstallHint {
            macos: "brew install helm",
            linux: "sudo snap install helm --classic",
            windows: "winget install Helm.Helm",
        },
    },
    Tool {
        name: "Git",
        program: "git",
        version_args: &["--version"],
        min: (2, 30),
        cloud_only: false,
        install: InstallHint { macos: "xcode-select --install", linux: "sudo apt install git", windows: "winget install Git.Git" },
    },
    Tool {
        name: "OpenSSH",
        program: "ssh",
        version_args: &["-V"],
        min: (8, 0),
        cloud_only: false,
        install: InstallHint {
            macos: "preinstalled",
            linux: "sudo apt install openssh-client",
            windows: "Settings → Optional features → OpenSSH Client",
        },
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolState {
    Ok { version: (u32, u32) },
    Outdated { version: (u32, u32) },
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    pub tool: Tool,
    pub state: ToolState,
}

impl ToolStatus {
    #[inline]
    pub fn is_ok(&self) -> bool {
        matches!(self.state, ToolState::Ok { .. })
    }
}

/// First `major.minor` found in text (handles `v1.2.3`, `OpenSSH_9.6p1`, ...).
pub fn parse_version(text: &str) -> Option<(u32, u32)> {
    let bytes = text.as_bytes();
    (0..bytes.len()).find_map(|i| {
        if !bytes[i].is_ascii_digit() || (i > 0 && bytes[i - 1].is_ascii_digit()) {
            return None;
        }
        let rest = &text[i..];
        let mut it = rest.splitn(3, |c: char| !c.is_ascii_digit());
        let major = it.next()?.parse().ok()?;
        // require a '.' separator right after major
        let sep = rest.as_bytes().get(rest.find(|c: char| !c.is_ascii_digit())?)?;
        if *sep != b'.' {
            return None;
        }
        let minor = it.next()?.parse().ok()?;
        Some((major, minor))
    })
}

pub fn check(tool: &Tool) -> ToolStatus {
    let out = host_command(tool.program)
        .args(tool.version_args)
        .stdin(Stdio::null())
        .output();
    let state = match out {
        Ok(o) => {
            // `ssh -V` prints to stderr.
            let text = if o.stdout.is_empty() { o.stderr } else { o.stdout };
            match parse_version(&String::from_utf8_lossy(&text)) {
                Some(v) if v >= tool.min => ToolState::Ok { version: v },
                Some(v) => ToolState::Outdated { version: v },
                None if o.status.success() => ToolState::Ok { version: (0, 0) },
                None => ToolState::Missing,
            }
        }
        Err(_) => ToolState::Missing,
    };
    ToolStatus { tool: *tool, state }
}

/// Checks all tools. `cloud` includes OpenTofu.
pub fn check_all(cloud: bool) -> Vec<ToolStatus> {
    TOOLS.iter().filter(|t| cloud || !t.cloud_only).map(check).collect()
}

/// Windows needs WSL for Ansible.
pub fn wsl_available() -> bool {
    cfg!(windows) && Command::new("wsl").arg("--status").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn versions() {
        assert_eq!(parse_version("ansible-playbook [core 2.18.1]"), Some((2, 18)));
        assert_eq!(parse_version("OpenTofu v1.10.6\non darwin_arm64"), Some((1, 10)));
        assert_eq!(parse_version("OpenSSH_9.9p2, LibreSSL 3.3.6"), Some((9, 9)));
        assert_eq!(parse_version("Client Version: v1.34.1"), Some((1, 34)));
        assert_eq!(parse_version("git version 2.50.1 (Apple Git-155)"), Some((2, 50)));
        assert_eq!(parse_version("no digits"), None);
    }
}
