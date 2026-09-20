use crate::{contract::*, Result};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Default)]
struct JournalIndex {
    last: u64,
    bytes: u64,
    ids: HashMap<String, (u64, String)>,
}
pub struct Store {
    pub root: PathBuf,
    pub sessions: BTreeMap<String, Session>,
    journals: Mutex<HashMap<String, JournalIndex>>,
}
pub fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}
pub fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", crate::id()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    f.write_all(&serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        private_dir(&root.join("sessions/v2"))?;
        private_dir(&root.join("agent-events"))?;
        // A handoff is a recoverable transaction: replay both snapshots before enrollment.
        let transaction = root.join("sessions/v2/handoff.pending");
        if transaction.exists() {
            let pair: Vec<Session> =
                serde_json::from_slice(&fs::read(&transaction).map_err(|e| e.to_string())?)
                    .map_err(|_| "invalid handoff transaction")?;
            if pair.len() != 2 || pair.iter().any(|s| !safe_id(&s.session_id)) {
                return Err("invalid handoff transaction".into());
            }
            for s in pair {
                atomic_json(
                    &root
                        .join("sessions/v2")
                        .join(format!("{}.json", s.session_id)),
                    &s,
                )?;
            }
            fs::remove_file(transaction).map_err(|e| e.to_string())?;
        }
        let mut store = Self {
            root,
            sessions: BTreeMap::new(),
            journals: Mutex::new(HashMap::new()),
        };
        for entry in fs::read_dir(store.root.join("sessions/v2")).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let mut s: Session =
                serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("invalid session registry: {e}"))?;
            if s.version != VERSION || !safe_id(&s.session_id) {
                return Err("unsupported/corrupt session registry".into());
            }
            // Reconcile journal writes that reached disk before their snapshot did.
            let replay = store.events(&s.session_id, 0);
            let mut corrupt = false;
            match replay {
                Ok(events) => {
                    if s.last_event_sequence > events.last().map(|e| e.sequence).unwrap_or(0) {
                        corrupt = true;
                    }
                    for event in events {
                        if s.apply_event(&event).is_err() {
                            corrupt = true;
                            break;
                        }
                    }
                }
                Err(_) => corrupt = true,
            }
            if corrupt {
                s.cleanly_detached = false;
                s.state = State::Failed;
                s.capabilities
                    .gaps
                    .push("Session journal requires reconciliation".into());
            } else if s.live() {
                s.state = State::Disconnected;
            }
            // Interrupted admission is uncertain, never blindly re-execute it.
            for receipt in s.message_receipts.values_mut() {
                if receipt == "transport_accepted" {
                    *receipt = "uncertain".into();
                }
            }
            store.save(&s)?;
            store.sessions.insert(s.session_id.clone(), s);
        }
        Ok(store)
    }
    pub fn save(&self, session: &Session) -> Result<()> {
        if !safe_id(&session.session_id) {
            return Err("invalid session id".into());
        }
        atomic_json(
            &self
                .root
                .join("sessions/v2")
                .join(format!("{}.json", session.session_id)),
            session,
        )
    }
    pub fn insert(&mut self, s: Session) -> Result<()> {
        if self.sessions.contains_key(&s.session_id) {
            return Err("session already exists".into());
        }
        if s.native_session_id.is_some()
            && self.sessions.values().any(|x| {
                x.live() && x.runtime == s.runtime && x.native_session_id == s.native_session_id
            })
        {
            return Err("native conversation is already bound to a session".into());
        }
        if s.primary
            && self
                .sessions
                .values()
                .any(|x| x.role == s.role && x.primary && x.live())
        {
            return Err("role already has a primary session; use explicit handoff".into());
        }
        self.save(&s)?;
        self.sessions.insert(s.session_id.clone(), s);
        Ok(())
    }
    pub fn update(&mut self, s: Session) -> Result<()> {
        if !self.sessions.contains_key(&s.session_id) {
            return Err("unknown session".into());
        }
        if s.live()
            && s.native_session_id.is_some()
            && self.sessions.values().any(|other| {
                other.session_id != s.session_id
                    && other.live()
                    && other.runtime == s.runtime
                    && other.native_session_id == s.native_session_id
            })
        {
            return Err("native conversation is already bound to a session".into());
        }
        if s.primary
            && s.live()
            && self
                .sessions
                .values()
                .any(|x| x.session_id != s.session_id && x.role == s.role && x.primary && x.live())
        {
            return Err("role primary lease is held".into());
        }
        self.save(&s)?;
        self.sessions.insert(s.session_id.clone(), s);
        Ok(())
    }
    pub fn handoff(&mut self, old: Session, new: Session) -> Result<()> {
        if old.role != new.role || old.primary || old.state != State::Stopped {
            return Err("invalid handoff transaction".into());
        }
        let path = self.root.join("sessions/v2/handoff.pending");
        atomic_json(&path, &vec![old.clone(), new.clone()])?;
        self.update(old)?;
        self.update(new)?;
        fs::remove_file(path).map_err(|e| e.to_string())
    }
    pub fn get(&self, id: &str) -> Result<Session> {
        self.sessions
            .get(id)
            .cloned()
            .ok_or_else(|| "unknown session".into())
    }
    pub fn events(&self, id: &str, after: u64) -> Result<Vec<Event>> {
        if !safe_id(id) {
            return Err("invalid session id".into());
        }
        let path = self.root.join("agent-events").join(format!("{id}.jsonl"));
        let mut f = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e.to_string()),
        };
        if f.metadata().map_err(|e| e.to_string())?.len() > 0 {
            f.seek(SeekFrom::End(-1)).map_err(|e| e.to_string())?;
            let mut end = [0];
            f.read_exact(&mut end).map_err(|e| e.to_string())?;
            if end[0] != b'\n' {
                return Err("event journal has an incomplete record; repair before append".into());
            }
            f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        }
        let mut events = Vec::new();
        let mut sequence = 0;
        for line in BufReader::new(f).split(b'\n') {
            let bytes = line.map_err(|e| e.to_string())?;
            if bytes.is_empty() {
                continue;
            }
            // A torn final record is unavailable evidence, not a valid event.
            let event: Event = serde_json::from_slice(&bytes)
                .map_err(|_| "event journal has an incomplete record; repair before append")?;
            if event.version != VERSION || event.session_id != id || event.sequence != sequence + 1
            {
                return Err("event journal ordering or identity is corrupt".into());
            }
            sequence = event.sequence;
            if event.sequence > after {
                events.push(event);
            }
        }
        Ok(events)
    }
    pub fn last_sequence(&self, id: &str) -> Result<u64> {
        let mut journals = self
            .journals
            .lock()
            .map_err(|_| "journal index unavailable")?;
        if !journals.contains_key(id) {
            let mut index = JournalIndex::default();
            for old in self.events(id, 0)? {
                index.last = old.sequence;
                index
                    .ids
                    .insert(old.event_id.clone(), (old.sequence, fingerprint(&old)));
            }
            index.bytes = fs::metadata(self.root.join("agent-events").join(format!("{id}.jsonl")))
                .map(|m| m.len())
                .unwrap_or(0);
            journals.insert(id.into(), index);
        }
        Ok(journals[id].last)
    }
    pub fn append(&self, mut event: Event) -> Result<Event> {
        if event.version != VERSION || !safe_id(&event.session_id) || !safe_id(&event.event_id) {
            return Err("invalid event envelope".into());
        }
        if !self.sessions.contains_key(&event.session_id) {
            return Err("unknown event session".into());
        }
        let mut journals = self
            .journals
            .lock()
            .map_err(|_| "journal index unavailable")?;
        if !journals.contains_key(&event.session_id) {
            let mut index = JournalIndex::default();
            for old in self.events(&event.session_id, 0)? {
                index.last = old.sequence;
                index
                    .ids
                    .insert(old.event_id.clone(), (old.sequence, fingerprint(&old)));
            }
            index.bytes = fs::metadata(
                self.root
                    .join("agent-events")
                    .join(format!("{}.jsonl", event.session_id)),
            )
            .map(|m| m.len())
            .unwrap_or(0);
            journals.insert(event.session_id.clone(), index);
        }
        let index = journals.get_mut(&event.session_id).unwrap();
        let path = self
            .root
            .join("agent-events")
            .join(format!("{}.jsonl", event.session_id));
        if fs::metadata(&path).map(|m| m.len()).unwrap_or(0) != index.bytes {
            return Err(
                "event journal changed outside the supervisor; reconcile before append".into(),
            );
        }
        let hash = fingerprint(&event);
        if let Some((sequence, old_hash)) = index.ids.get(&event.event_id) {
            if old_hash != &hash {
                return Err("event id reused with different content".into());
            }
            return self
                .events(&event.session_id, sequence - 1)?
                .into_iter()
                .next()
                .ok_or_else(|| "replayed event unavailable".into());
        }
        if event.sequence != 0 && event.sequence != index.last + 1 {
            return Err("out-of-order event".into());
        }
        event.sequence = index.last + 1;
        event.timestamp = crate::now();
        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        let mut bytes = serde_json::to_vec(&event).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        f.write_all(&bytes)
            .and_then(|_| f.sync_data())
            .map_err(|e| e.to_string())?;
        index.last = event.sequence;
        index.bytes += bytes.len() as u64;
        index
            .ids
            .insert(event.event_id.clone(), (event.sequence, hash));
        // The detailed journal is authoritative; spine is an observational projection.
        let session = self.sessions.get(&event.session_id).unwrap();
        let projection = serde_json::json!({"ts":event.timestamp,"event":format!("agent.{}",event.event_type),"role":session.role,"runtime":session.runtime,"event_id":event.event_id,"session_id":event.session_id,"sequence":event.sequence,"trace_id":event.trace_id});
        if let Ok(mut spine) = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(self.root.join("chorus.log"))
        {
            let _ = writeln!(spine, "{projection}");
        }
        Ok(event)
    }
}

fn fingerprint(event: &Event) -> String {
    let mut value = serde_json::to_value(event).unwrap();
    value.as_object_mut().unwrap().remove("sequence");
    value.as_object_mut().unwrap().remove("timestamp");
    crate::digest(&serde_json::to_vec(&value).unwrap())
}

pub fn event(session_id: &str, kind: &str, data: serde_json::Value) -> Event {
    Event {
        version: VERSION,
        session_id: session_id.into(),
        event_id: crate::id(),
        sequence: 0,
        timestamp: String::new(),
        event_type: kind.into(),
        native_session_id: None,
        turn_id: None,
        tool_call_id: None,
        message_id: None,
        trace_id: None,
        data,
    }
}
