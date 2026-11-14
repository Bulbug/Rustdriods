//! Checks whether this PC can run a hardware-accelerated Android emulator,
//! and reads how much RAM it has (used to suggest a device preset).
use crate::runner::no_window;
use std::process::Command;

pub struct VirtInfo {
    pub ok: bool,
    pub message: String,
    pub ram_mb: Option<u64>,
}

pub fn check() -> VirtInfo {
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-Command",
        "(Get-CimInstance Win32_ComputerSystem).HypervisorPresent; \
         (Get-CimInstance Win32_Processor | Select-Object -First 1).VirtualizationFirmwareEnabled; \
         (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
    ]);
    no_window(&mut cmd);

    match cmd.output() {
        Ok(o) => parse(&String::from_utf8_lossy(&o.stdout)),
        Err(e) => VirtInfo { ok: false, message: format!("Could not check: {e}"), ram_mb: None },
    }
}

/// Reads the three lines PowerShell prints: hypervisor, BIOS virtualization, RAM in bytes.
/// (If the RAM line is missing we still report virtualization.)
fn parse(text: &str) -> VirtInfo {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let ram_mb = lines.last().and_then(|l| l.parse::<u64>().ok()).map(|bytes| bytes / 1_048_576);
    let skip = if ram_mb.is_some() { 1 } else { 0 };
    let n = lines.len();
    if n < 2 + skip {
        return VirtInfo { ok: false, message: "Could not read virtualization info".into(), ram_mb };
    }
    let hypervisor = lines[n - 2 - skip].eq_ignore_ascii_case("true");
    let firmware = lines[n - 1 - skip].eq_ignore_ascii_case("true");

    let (ok, message) = if hypervisor {
        (true, "Ready (Windows hypervisor is active)".to_string())
    } else if firmware {
        (false, "BIOS virtualization is on, but enable \"Windows Hypervisor Platform\" in Windows Features, then restart".to_string())
    } else {
        (false, "Turn on virtualization (Intel VT-x / AMD SVM) in your BIOS".to_string())
    };
    VirtInfo { ok, message, ram_mb }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_all_three_lines() {
        let v = parse("True\r\nFalse\r\n17036599296\r\n");
        assert!(v.ok);
        assert_eq!(v.ram_mb, Some(16247));
    }

    #[test]
    fn works_without_the_ram_line() {
        let v = parse("False\nTrue\n");
        assert!(!v.ok);
        assert!(v.message.contains("Windows Hypervisor Platform"));
        assert_eq!(v.ram_mb, None);
    }
}
