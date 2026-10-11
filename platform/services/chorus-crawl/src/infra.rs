// @domain: infrastructure — the infrastructure crawl: machines and their disks (#4485)
//! #4485 — the infrastructure crawl. Pure decisions only: which df lines are a
//! machine's own disks, and which readings are findings. main does the I/O
//! (df, scutil, sysctl, sw_vers, ssh, the door), the way the services crawl does.
//!
//! Measured against the machines themselves, never config (Jeff's AC). A
//! machine the crawl could not walk reads UNMEASURED, never "disk ok".

/// One mounted volume, as `df -kP` reports it, in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskReading {
    pub mount: String,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    /// what df says is free. On APFS the capacity is the shared container's, so
    /// used / capacity understates a volume; df's own percent uses this (Wren).
    pub available_bytes: u64,
}

impl DiskReading {
    /// Percent full exactly as df prints it: used / (used + available), rounded
    /// up. Computed by the reader, never stored.
    pub fn percent(&self) -> u64 {
        df_percent(self.used_bytes, self.available_bytes)
    }
}

/// The sealed system snapshots macOS mounts beside the boot volume: not disks
/// anyone fills, so not rows.
const SYSTEM_SNAPSHOTS: &[&str] = &[
    "/System/Volumes/VM",
    "/System/Volumes/Preboot",
    "/System/Volumes/Update",
    "/System/Volumes/xarts",
    "/System/Volumes/iSCPreboot",
    "/System/Volumes/Hardware",
];

/// The volumes a full disk always matters on: boot and the user data volume.
pub const SYSTEM_VOLUMES: &[&str] = &["/", "/System/Volumes/Data"];

/// `df -kP` output → the machine's own disks. Kept: a local device (`/dev/…`)
/// that is not a sealed snapshot. Dropped: devfs, automounts, and network
/// mounts, which are another machine's disk and are counted there.
pub fn parse_df(text: &str) -> Vec<DiskReading> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 6 || !f[0].starts_with("/dev/") {
                return None;
            }
            // a mount point may contain spaces: everything after the capacity column
            let mount = f[5..].join(" ");
            if SYSTEM_SNAPSHOTS.contains(&mount.as_str()) {
                return None;
            }
            let blocks: u64 = f[1].parse().ok()?;
            let used: u64 = f[2].parse().ok()?;
            let avail: u64 = f[3].parse().ok()?;
            Some(DiskReading { mount, capacity_bytes: blocks * 1024, used_bytes: used * 1024, available_bytes: avail * 1024 })
        })
        .collect()
}

/// What the crawl got from one machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Walk {
    Walked(Vec<DiskReading>),
    /// could not walk it: the reason, as the walk saw it
    Unwalkable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    DiskFull { machine: String, mount: String, percent: u64 },
    /// the machine's disks are unmeasured: a finding, never a pass
    Unmeasured { machine: String, reason: String },
}

/// A disk that can fail: a system volume, or one a store sits on.
fn watched(d: &DiskReading, store_mounts: &[&str]) -> bool {
    SYSTEM_VOLUMES.contains(&d.mount.as_str()) || store_mounts.contains(&d.mount.as_str())
}

/// The full disks that are not findings: archive drives, reported with their percent.
pub fn full_archives<'a>(disks: &'a [DiskReading], store_mounts: &[&str]) -> Vec<&'a DiskReading> {
    disks.iter().filter(|d| d.percent() > FULL_PERCENT && !watched(d, store_mounts)).collect()
}

/// Over this percent a watched disk is a finding.
pub const FULL_PERCENT: u64 = 90;

/// The findings for one machine. A disk can fail when it is a system volume or
/// a store sits on it (`store_mounts`, from the onDisk links). A full disk with
/// neither is an archive drive, filled on purpose: reported, not a finding
/// (Wren, 2026-10-11 14:04 — worked out from the links, no name list).
pub fn disk_findings(machine: &str, walk: &Walk, store_mounts: &[&str]) -> Vec<Finding> {
    match walk {
        Walk::Unwalkable(reason) => vec![Finding::Unmeasured { machine: machine.into(), reason: reason.clone() }],
        Walk::Walked(disks) => disks
            .iter()
            .filter(|d| watched(d, store_mounts))
            .filter(|d| d.percent() > FULL_PERCENT)
            .map(|d| Finding::DiskFull { machine: machine.into(), mount: d.mount.clone(), percent: d.percent() })
            .collect(),
    }
}

/// What one walk asks a machine, local or over ssh: its own name, model and
/// OS, then df. One command so a machine is walked whole or not at all.
pub const WALK_CMD: &str = "echo HOST:$(scutil --get LocalHostName); echo MODEL:$(sysctl -n hw.model); \
     echo OS:$(sw_vers -productVersion); echo ---DF---; df -kP";

/// A machine's measured facts. A field the machine did not answer stays empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineFacts {
    pub hostname: String,
    pub hardware_model: String,
    pub os_version: String,
}

/// The output of [`WALK_CMD`] → the machine's facts and its disks. No df
/// section means the walk did not finish: an error, so the machine reads
/// UNMEASURED rather than "no disks".
pub fn parse_walk(text: &str) -> Result<(MachineFacts, Vec<DiskReading>), String> {
    let (head, df) = text.split_once("---DF---").ok_or("the walk returned no df section")?;
    let val = |key: &str| {
        head.lines().find_map(|l| l.strip_prefix(key)).map(|v| v.trim().to_string()).unwrap_or_default()
    };
    let facts = MachineFacts { hostname: val("HOST:"), hardware_model: val("MODEL:"), os_version: val("OS:") };
    let disks = parse_df(df);
    if disks.is_empty() {
        return Err("df listed no local disk".into());
    }
    Ok((facts, disks))
}

/// A Disk row's name: the machine and the mount, slugged the way the door
/// names rows (lowercase, hyphens). `/` is the root volume. Bare: the door
/// mints the `disk-` prefix itself and refuses a name that already has it.
pub fn disk_name(machine: &str, mount: &str) -> String {
    let slug: String = mount
        .trim_matches('/')
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("{machine}-{}", if slug.is_empty() { "root" } else { &slug })
}

/// One row as the door holds it: its kind (Machine or Disk), name and fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: &'static str,
    pub name: String,
    pub fields: Vec<(String, String)>,
}

impl Row {
    pub fn get(&self, k: &str) -> &str {
        self.fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.as_str()).unwrap_or("")
    }
}

/// The fields this crawl owns, per kind. Anything else on a row belongs to
/// another writer and is put back untouched. `comment` is owned on Machine so
/// the hand-typed prose goes (Wren, 4): the crawl writes measured fields only.
pub fn owned_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "Machine" => &["hostname", "hardwareModel", "osVersion", "comment"],
        // #4422 — the stores crawl writes PersistenceStore rows through the same plan
        "PersistenceStore" => &[
            "label", "storeType", "storeNamespace", "storeRole", "roleSource", "inStore", "onDisk", "storeServedBy",
            "storeRecordCount", "storeStatus", "writeMode", "roleBasis",
        ],
        _ => &["label", "onMachine", "mountPoint", "capacityBytes", "usedBytes", "availableBytes"],
    }
}

pub fn machine_row(machine: &str, facts: &MachineFacts, ts: &str) -> Row {
    let mut fields = Vec::new();
    for (k, v) in [("hostname", &facts.hostname), ("hardwareModel", &facts.hardware_model), ("osVersion", &facts.os_version)] {
        if !v.is_empty() {
            fields.push((k.to_string(), v.clone()));
        }
    }
    fields.push(("lastObserved".into(), ts.into()));
    Row { kind: "Machine", name: machine.into(), fields }
}

pub fn disk_row(machine: &str, d: &DiskReading, ts: &str) -> Row {
    Row {
        kind: "Disk",
        name: disk_name(machine, &d.mount),
        fields: vec![
            ("label".into(), format!("{} ({machine})", d.mount)),
            ("onMachine".into(), machine.into()),
            ("mountPoint".into(), d.mount.clone()),
            ("capacityBytes".into(), d.capacity_bytes.to_string()),
            ("usedBytes".into(), d.used_bytes.to_string()),
            ("availableBytes".into(), d.available_bytes.to_string()),
            ("lastObserved".into(), ts.into()),
        ],
    }
}

/// lastObserved alone is refreshed only after this long, so a second run in a
/// row writes nothing (AC2) while a row still says when it was last seen.
pub const REFRESH_AFTER_SECS: u64 = 12 * 3600;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub create: Vec<Row>,
    pub update: Vec<Row>,
    pub unchanged: usize,
}

/// df's percent: used / (used + available), rounded up.
pub fn df_percent(used: u64, available: u64) -> u64 {
    let total = used + available;
    if total == 0 { 0 } else { (used * 100).div_ceil(total) }
}

fn percent_of(r: &Row) -> Option<u64> {
    let used: u64 = r.get("usedBytes").parse().ok()?;
    let avail: u64 = r.get("availableBytes").parse().ok()?;
    Some(df_percent(used, avail))
}

/// What to write. Changed-only: a disk's used bytes move every minute, so a
/// disk counts as changed only when its whole percent moves. A Machine row the
/// door does not hold is never created here — machines are the seed's, with
/// their parent link; the crawl only measures them.
pub fn plan(desired: Vec<Row>, current: &[Row], now_secs: u64, iso_secs: &dyn Fn(&str) -> Option<u64>) -> Plan {
    let mut p = Plan::default();
    for d in desired {
        match current.iter().find(|c| c.name == d.name && c.kind == d.kind) {
            None if d.kind == "Machine" => {}
            None => p.create.push(d),
            Some(c) => {
                let owned = owned_fields(d.kind);
                let differs = owned.iter().any(|k| match *k {
                    // used and free move every minute; the row changes when df's percent does
                    "usedBytes" | "availableBytes" => percent_of(c) != percent_of(&d),
                    _ => c.get(k) != d.get(k),
                });
                let old = iso_secs(c.get("lastObserved")).map_or(true, |t| now_secs.saturating_sub(t) >= REFRESH_AFTER_SECS);
                if differs || old {
                    let mut fields: Vec<(String, String)> = c
                        .fields
                        .iter()
                        .filter(|(k, v)| !v.is_empty() && k != "lastObserved" && !owned.contains(&k.as_str()))
                        .cloned()
                        .collect();
                    fields.extend(d.fields.iter().filter(|(_, v)| !v.is_empty()).cloned());
                    p.update.push(Row { kind: d.kind, name: d.name, fields });
                } else {
                    p.unchanged += 1;
                }
            }
        }
    }
    p
}
