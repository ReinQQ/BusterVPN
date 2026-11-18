use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use windows::core::{BSTR, HRESULT};
use windows::Win32::Foundation::VARIANT_TRUE;
use windows::Win32::NetworkManagement::WindowsFirewall::{
    INetFwPolicy2, INetFwRule, INetFwRules, NetFwPolicy2, NetFwRule,
    NET_FW_ACTION, NET_FW_ACTION_ALLOW, NET_FW_ACTION_BLOCK,
    NET_FW_IP_PROTOCOL_ANY, NET_FW_IP_PROTOCOL_TCP, NET_FW_IP_PROTOCOL_UDP,
    NET_FW_PROFILE2_ALL, NET_FW_PROFILE2_DOMAIN, NET_FW_PROFILE2_PRIVATE, NET_FW_PROFILE2_PUBLIC,
    NET_FW_PROFILE_TYPE2, NET_FW_RULE_DIR_OUT,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use crate::error::WindowsError;

pub const KS_RULE_ALLOW_LOOPBACK: &str = "BusterVPN_KS_AllowLoopback";
pub const KS_RULE_ALLOW_SERVER: &str = "BusterVPN_KS_AllowServer";
pub const KS_RULE_ALLOW_TUN: &str = "BusterVPN_KS_AllowTun";
pub const KS_RULE_BLOCK_ALL: &str = "BusterVPN_KS_BlockAll";
pub const KS_RULE_CORE: &str = "BusterVPN_KS_Core";
pub const KS_RULE_LOOPBACK: &str = "BusterVPN_KS_Loopback";
pub const KS_RULE_SERVER: &str = "BusterVPN_KS_Server";
pub const KS_RULE_DNS_UDP: &str = "BusterVPN_KS_DNS_UDP";
pub const KS_RULE_DNS_TCP: &str = "BusterVPN_KS_DNS_TCP";
pub const KS_RULE_DHCP: &str = "BusterVPN_KS_DHCP";

pub const KS_ALL_RULE_NAMES: &[&str] = &[
    KS_RULE_ALLOW_LOOPBACK,
    KS_RULE_ALLOW_SERVER,
    KS_RULE_ALLOW_TUN,
    KS_RULE_BLOCK_ALL,
    KS_RULE_CORE,
    KS_RULE_LOOPBACK,
    KS_RULE_SERVER,
    KS_RULE_DNS_UDP,
    KS_RULE_DNS_TCP,
    KS_RULE_DHCP,
];

const PROFILES: &[NET_FW_PROFILE_TYPE2] = &[
    NET_FW_PROFILE2_DOMAIN,
    NET_FW_PROFILE2_PRIVATE,
    NET_FW_PROFILE2_PUBLIC,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitchJournal {
    pub domain_action: i32,
    pub private_action: i32,
    pub public_action: i32,
    pub enabled_at_utc: DateTime<Utc>,
}

pub struct FirewallKillSwitch {
    journal_path: PathBuf,
}

impl FirewallKillSwitch {
    pub fn new(journal_path: impl Into<PathBuf>) -> Self {
        Self {
            journal_path: journal_path.into(),
        }
    }

    /// Enables the fail-closed Kill Switch:
    /// 1. Snapshots current profile default outbound actions to an atomic journal.
    /// 2. Cleans up any existing BusterVPN_KS_ rules.
    /// 3. Adds narrowly-scoped allow rules (Loopback, VPN Server IP, Core Executable, DNS/DHCP).
    /// 4. Sets default outbound action for all profiles to `NET_FW_ACTION_BLOCK`.
    pub fn enable(
        &self,
        core_executable: Option<&Path>,
        server_ips: &[String],
    ) -> Result<(), WindowsError> {
        let _com = ComGuard::new()?;

        unsafe {
            let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)?;
            let rules: INetFwRules = policy.Rules()?;

            // 1. Snapshot previous firewall outbound actions
            let journal = KillSwitchJournal {
                domain_action: policy.get_DefaultOutboundAction(NET_FW_PROFILE2_DOMAIN)?.0,
                private_action: policy.get_DefaultOutboundAction(NET_FW_PROFILE2_PRIVATE)?.0,
                public_action: policy.get_DefaultOutboundAction(NET_FW_PROFILE2_PUBLIC)?.0,
                enabled_at_utc: Utc::now(),
            };
            self.write_journal(&journal)?;

            // 2. Remove stale rules first
            self.cleanup_rules_internal(&rules);

            // 3. Add allow rules
            // Rule 1: Loopback allow (essential for 127.0.0.1:10808 / 10809 proxy listeners and localhost)
            self.add_rule(
                &rules,
                KS_RULE_ALLOW_LOOPBACK,
                NET_FW_ACTION_ALLOW,
                NET_FW_IP_PROTOCOL_ANY.0 as i32,
                None,
                None,
                None,
                Some("127.0.0.1/255.0.0.0,::1"),
            )?;

            // Rule 2: Target VPN Server IPs allow
            let valid_ips: Vec<&str> = server_ips
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if !valid_ips.is_empty() {
                let ips_joined = valid_ips.join(",");
                self.add_rule(
                    &rules,
                    KS_RULE_ALLOW_SERVER,
                    NET_FW_ACTION_ALLOW,
                    NET_FW_IP_PROTOCOL_ANY.0 as i32,
                    None,
                    None,
                    None,
                    Some(&ips_joined),
                )?;
            }

            // Rule 3: Allow Outbound on interface BusterVPN_TUN (172.19.0.0/30)
            self.add_rule(
                &rules,
                KS_RULE_ALLOW_TUN,
                NET_FW_ACTION_ALLOW,
                NET_FW_IP_PROTOCOL_ANY.0 as i32,
                None,
                None,
                None,
                Some("172.19.0.0/255.255.255.252"),
            )?;

            // Core Executable allow
            if let Some(exe) = core_executable {
                let exe_str = exe.to_string_lossy();
                self.add_rule(
                    &rules,
                    KS_RULE_CORE,
                    NET_FW_ACTION_ALLOW,
                    NET_FW_IP_PROTOCOL_ANY.0 as i32,
                    Some(&exe_str),
                    None,
                    None,
                    None,
                )?;
            }

            // DNS & DHCP allows (via svchost)
            let svchost = svchost_path();
            let svchost_str = svchost.to_string_lossy();
            self.add_rule(
                &rules,
                KS_RULE_DNS_UDP,
                NET_FW_ACTION_ALLOW,
                NET_FW_IP_PROTOCOL_UDP.0 as i32,
                Some(&svchost_str),
                None,
                Some("53"),
                None,
            )?;
            self.add_rule(
                &rules,
                KS_RULE_DNS_TCP,
                NET_FW_ACTION_ALLOW,
                NET_FW_IP_PROTOCOL_TCP.0 as i32,
                Some(&svchost_str),
                None,
                Some("53"),
                None,
            )?;
            self.add_rule(
                &rules,
                KS_RULE_DHCP,
                NET_FW_ACTION_ALLOW,
                NET_FW_IP_PROTOCOL_UDP.0 as i32,
                Some(&svchost_str),
                Some("68"),
                Some("67"),
                None,
            )?;

            // Rule 4: Outbound Action NET_FW_ACTION_BLOCK
            self.add_rule(
                &rules,
                KS_RULE_BLOCK_ALL,
                NET_FW_ACTION_BLOCK,
                NET_FW_IP_PROTOCOL_ANY.0 as i32,
                None,
                None,
                None,
                None,
            )?;

            // 4. Block all other outbound traffic for Domain, Private, and Public profiles
            for profile in PROFILES {
                policy.put_DefaultOutboundAction(*profile, NET_FW_ACTION_BLOCK)?;
            }

            tracing::warn!("Kill Switch enabled: all non-VPN outbound traffic is blocked by Windows Firewall");
            Ok(())
        }
    }

    /// Disables the Kill Switch and restores the previous default firewall outbound action.
    pub fn disable(&self) -> Result<(), WindowsError> {
        let _com = ComGuard::new()?;

        unsafe {
            let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)?;
            let rules: INetFwRules = policy.Rules()?;

            // Remove all BusterVPN_KS_ rules
            self.cleanup_rules_internal(&rules);

            // Restore defaults from journal or fallback to Allow
            let (domain_act, private_act, public_act) = if self.journal_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&self.journal_path) {
                    if let Ok(journal) = serde_json::from_str::<KillSwitchJournal>(&content) {
                        (
                            NET_FW_ACTION(journal.domain_action),
                            NET_FW_ACTION(journal.private_action),
                            NET_FW_ACTION(journal.public_action),
                        )
                    } else {
                        (NET_FW_ACTION_ALLOW, NET_FW_ACTION_ALLOW, NET_FW_ACTION_ALLOW)
                    }
                } else {
                    (NET_FW_ACTION_ALLOW, NET_FW_ACTION_ALLOW, NET_FW_ACTION_ALLOW)
                }
            } else {
                (NET_FW_ACTION_ALLOW, NET_FW_ACTION_ALLOW, NET_FW_ACTION_ALLOW)
            };

            policy.put_DefaultOutboundAction(NET_FW_PROFILE2_DOMAIN, domain_act)?;
            policy.put_DefaultOutboundAction(NET_FW_PROFILE2_PRIVATE, private_act)?;
            policy.put_DefaultOutboundAction(NET_FW_PROFILE2_PUBLIC, public_act)?;

            if self.journal_path.exists() {
                let _ = std::fs::remove_file(&self.journal_path);
            }

            tracing::info!("Kill Switch disabled and firewall defaults restored");
            Ok(())
        }
    }

    /// Removes all residual `BusterVPN_KS_` rules.
    pub fn cleanup_all(&self) -> Result<(), WindowsError> {
        let _com = ComGuard::new()?;
        unsafe {
            let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)?;
            let rules: INetFwRules = policy.Rules()?;
            self.cleanup_rules_internal(&rules);
        }
        Ok(())
    }

    /// Recovers firewall settings if an orphaned recovery journal was found on startup.
    pub fn recover_if_needed(&self) -> Result<bool, WindowsError> {
        if !self.journal_path.exists() {
            return Ok(false);
        }

        tracing::warn!(
            "Found stale Kill Switch journal at '{}'. Restoring firewall defaults...",
            self.journal_path.display()
        );

        self.disable()?;
        Ok(true)
    }

    fn cleanup_rules_internal(&self, rules: &INetFwRules) {
        for name in KS_ALL_RULE_NAMES {
            unsafe {
                let _ = rules.Remove(&BSTR::from(*name));
            }
        }
    }

    fn add_rule(
        &self,
        rules: &INetFwRules,
        name: &str,
        action: NET_FW_ACTION,
        protocol: i32,
        app_name: Option<&str>,
        local_ports: Option<&str>,
        remote_ports: Option<&str>,
        remote_addrs: Option<&str>,
    ) -> Result<(), WindowsError> {
        unsafe {
            let rule: INetFwRule = CoCreateInstance(&NetFwRule, None, CLSCTX_INPROC_SERVER)?;

            rule.SetName(&BSTR::from(name))?;
            rule.SetDescription(&BSTR::from("Managed by BusterVPN Kill Switch"))?;
            rule.SetGrouping(&BSTR::from("BusterVPN"))?;
            rule.SetEnabled(VARIANT_TRUE)?;
            rule.SetDirection(NET_FW_RULE_DIR_OUT)?;
            rule.SetAction(action)?;
            rule.SetProfiles(NET_FW_PROFILE2_ALL.0 as i32)?;
            rule.SetProtocol(protocol)?;

            if let Some(app) = app_name {
                rule.SetApplicationName(&BSTR::from(app))?;
            }
            if let Some(lp) = local_ports {
                rule.SetLocalPorts(&BSTR::from(lp))?;
            }
            if let Some(rp) = remote_ports {
                rule.SetRemotePorts(&BSTR::from(rp))?;
            }
            if let Some(ra) = remote_addrs {
                rule.SetRemoteAddresses(&BSTR::from(ra))?;
            }

            rules.Add(&rule)?;
            Ok(())
        }
    }

    fn write_journal(&self, journal: &KillSwitchJournal) -> Result<(), WindowsError> {
        if let Some(parent) = self.journal_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let temp_path = self.journal_path.with_extension("tmp");
        let json = serde_json::to_string_pretty(journal)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        std::fs::write(&temp_path, json)?;
        std::fs::rename(&temp_path, &self.journal_path)?;
        Ok(())
    }
}

fn svchost_path() -> PathBuf {
    let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    PathBuf::from(sys_root).join("System32").join("svchost.exe")
}

/// RAII Guard for COM initialization on the current thread.
struct ComGuard;

impl ComGuard {
    fn new() -> Result<Self, windows::core::Error> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            // S_OK (0), S_FALSE (1, already initialized), or RPC_E_CHANGED_MODE (0x80010106) are all benign
            if hr.is_err() && hr != HRESULT(1) && hr != HRESULT(0x80010106u32 as i32) {
                return Err(windows::core::Error::from(hr));
            }
            Ok(Self)
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firewall_rule_constants() {
        assert_eq!(KS_ALL_RULE_NAMES.len(), 10);
        assert!(KS_ALL_RULE_NAMES.contains(&"BusterVPN_KS_AllowLoopback"));
        assert!(KS_ALL_RULE_NAMES.contains(&"BusterVPN_KS_AllowServer"));
        assert!(KS_ALL_RULE_NAMES.contains(&"BusterVPN_KS_AllowTun"));
        assert!(KS_ALL_RULE_NAMES.contains(&"BusterVPN_KS_BlockAll"));
        assert!(KS_ALL_RULE_NAMES.contains(&"BusterVPN_KS_Loopback"));
        assert!(KS_ALL_RULE_NAMES.contains(&"BusterVPN_KS_Core"));
        assert!(KS_ALL_RULE_NAMES.contains(&"BusterVPN_KS_Server"));
    }
}
