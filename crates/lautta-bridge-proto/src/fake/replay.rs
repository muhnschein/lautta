// SPDX-License-Identifier: LGPL-2.1-or-later
//! Replays the shared golden sequences (`tests/bridge-contract/*.json`,
//! netvfs SPEC-v2 XT-7) against the fake bridge, so that this client and the
//! real bridge agree on the protocol (SPEC TST-2). The format and the matching
//! rules are in the README next to the files.

use super::{Consent, FakeBridge};
use crate::connect::Connection;
use crate::errors::BridgeError;
use crate::wire::{INTERFACE, OBJECT_PATH};
use futures::StreamExt;
use serde_json::{json, Value as Json};
use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;
use zbus::message::Type as MessageType;
use zbus::zvariant::{Array, Dict, Signature, Structure, StructureBuilder, Value};

const STEP_TIMEOUT: Duration = Duration::from_secs(5);
/// The one account the sequences run against.
pub const ACCOUNT: &str = "account:1";

/// A parsed sequence file.
#[derive(Debug)]
pub struct Sequence {
    pub description: String,
    pub consent: Consent,
    pub files: Vec<(String, String)>,
    pub steps: Vec<Step>,
}

#[derive(Debug)]
pub enum Step {
    Call {
        call: String,
        sig: String,
        args: Vec<Json>,
        outcome: Outcome,
    },
    Signal {
        signal: String,
        args: Vec<Json>,
    },
}

#[derive(Debug)]
pub enum Outcome {
    Reply(Vec<Json>),
    Error(String),
}

fn text<'a>(v: &'a Json, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("missing string \"{key}\""))
}

fn list(v: &Json, key: &str) -> Result<Vec<Json>, String> {
    v.get(key)
        .and_then(Json::as_array)
        .cloned()
        .ok_or_else(|| format!("missing array \"{key}\""))
}

fn parse_step(step: &Json) -> Result<Step, String> {
    if let Some(signal) = step.get("signal").and_then(Json::as_str) {
        return Ok(Step::Signal {
            signal: signal.to_owned(),
            args: list(step, "args")?,
        });
    }
    let outcome = match (step.get("reply"), step.get("error").and_then(Json::as_str)) {
        (Some(Json::Array(r)), None) => Outcome::Reply(r.clone()),
        (None, Some(e)) => Outcome::Error(e.to_owned()),
        _ => return Err("a call needs exactly one of \"reply\" and \"error\"".to_owned()),
    };
    Ok(Step::Call {
        call: text(step, "call")?.to_owned(),
        sig: text(step, "sig")?.to_owned(),
        args: list(step, "args")?,
        outcome,
    })
}

/// Parses a sequence file.
pub fn parse(source: &str) -> Result<Sequence, String> {
    let doc: Json = serde_json::from_str(source).map_err(|e| e.to_string())?;
    let consent = match text(&doc, "consent")? {
        "granted" => Consent::Granted,
        "denied" => Consent::Denied,
        "unknown" => Consent::Unknown,
        other => return Err(format!("unknown consent \"{other}\"")),
    };
    let files = doc
        .get("files")
        .and_then(Json::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let steps = list(&doc, "steps")?
        .iter()
        .enumerate()
        .map(|(i, s)| parse_step(s).map_err(|e| format!("step {}: {e}", i + 1)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Sequence {
        description: text(&doc, "description")?.to_owned(),
        consent,
        files,
        steps,
    })
}

// ---- value encoding ------------------------------------------------------

/// Splits a signature into its complete types (`"saya{sv}"` -> `s`, `ay`, `a{sv}`).
fn split_types(sig: &str) -> Result<Vec<&str>, String> {
    let bytes = sig.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let end = type_end(bytes, start)?;
        out.push(&sig[start..end]);
        start = end;
    }
    Ok(out)
}

fn type_end(bytes: &[u8], start: usize) -> Result<usize, String> {
    let mut i = start;
    while bytes.get(i) == Some(&b'a') {
        i += 1;
    }
    match bytes.get(i) {
        Some(b'(') | Some(b'{') => {
            let mut depth = 0usize;
            for (j, b) in bytes.iter().enumerate().skip(i) {
                match b {
                    b'(' | b'{' => depth += 1,
                    b')' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(j + 1);
                        }
                    }
                    _ => {}
                }
            }
            Err("unbalanced signature".to_owned())
        }
        Some(_) => Ok(i + 1),
        None => Err("truncated signature".to_owned()),
    }
}

fn bytes_of(v: &Json) -> Result<Vec<u8>, String> {
    if let Some(s) = v.get("bytes").and_then(Json::as_str) {
        return Ok(s.as_bytes().to_vec());
    }
    if let Some(h) = v.get("hex").and_then(Json::as_str) {
        return (0..h.len())
            .step_by(2)
            .map(|i| {
                h.get(i..i + 2)
                    .and_then(|p| u8::from_str_radix(p, 16).ok())
                    .ok_or_else(|| format!("bad hex \"{h}\""))
            })
            .collect();
    }
    Err(format!("expected {{\"bytes\"}} or {{\"hex\"}}, got {v}"))
}

fn number(v: &Json) -> Result<i64, String> {
    v.as_i64().ok_or_else(|| format!("expected an integer, got {v}"))
}

fn encode(t: &str, v: &Json) -> Result<Value<'static>, String> {
    let range = |n: i64| format!("{n} does not fit {t}");
    Ok(match t.as_bytes()[0] {
        b'y' => Value::U8(u8::try_from(number(v)?).map_err(|_| range(number(v).unwrap_or(0)))?),
        b'q' => Value::U16(u16::try_from(number(v)?).map_err(|_| range(number(v).unwrap_or(0)))?),
        b'u' => Value::U32(u32::try_from(number(v)?).map_err(|_| range(number(v).unwrap_or(0)))?),
        b'i' => Value::I32(i32::try_from(number(v)?).map_err(|_| range(number(v).unwrap_or(0)))?),
        b'x' => Value::I64(number(v)?),
        b'b' => Value::Bool(
            v.as_bool()
                .ok_or_else(|| format!("expected a boolean, got {v}"))?,
        ),
        b's' => Value::from(
            v.as_str()
                .ok_or_else(|| format!("expected a string, got {v}"))?
                .to_owned(),
        ),
        b'a' => encode_array(&t[1..], v)?,
        b'(' => {
            let fields = split_types(&t[1..t.len() - 1])?;
            let items = v.as_array().filter(|a| a.len() == fields.len());
            let items = items.ok_or_else(|| format!("expected {} fields, got {v}", fields.len()))?;
            let mut b = StructureBuilder::new();
            for (ft, item) in fields.iter().zip(items) {
                b = b.append_field(encode(ft, item)?);
            }
            Value::Structure(b.build())
        }
        _ => return Err(format!("unsupported type {t}")),
    })
}

fn encode_array(elem: &str, v: &Json) -> Result<Value<'static>, String> {
    if elem == "y" {
        return Ok(Value::from(bytes_of(v)?));
    }
    if let Some(kv) = elem.strip_prefix('{') {
        if kv == "sv}" {
            return encode_dict(v);
        }
        return Err(format!("unsupported dictionary {elem}"));
    }
    let sig = Signature::try_from(elem.to_owned()).map_err(|e| e.to_string())?;
    let mut arr = Array::new(sig);
    for item in v
        .as_array()
        .ok_or_else(|| format!("expected an array, got {v}"))?
    {
        arr.append(encode(elem, item)?).map_err(|e| e.to_string())?;
    }
    Ok(Value::Array(arr))
}

/// `a{sv}`: integers travel as `x`.
fn encode_dict(v: &Json) -> Result<Value<'static>, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| format!("expected an object, got {v}"))?;
    let mut dict = Dict::new(
        Signature::try_from("s").map_err(|e| e.to_string())?,
        Signature::try_from("v").map_err(|e| e.to_string())?,
    );
    for (k, item) in obj {
        let inner = match item {
            Json::Bool(_) => encode("b", item)?,
            Json::Number(_) => encode("x", item)?,
            Json::String(_) => encode("s", item)?,
            other => return Err(format!("unsupported option value {other}")),
        };
        dict.append(Value::from(k.clone()), Value::Value(Box::new(inner)))
            .map_err(|e| e.to_string())?;
    }
    Ok(Value::Dict(dict))
}

// ---- value decoding ------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        // Writing into a String cannot fail.
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn decode(v: &Value<'_>) -> Json {
    match v {
        Value::U8(n) => json!(n),
        Value::Bool(b) => json!(b),
        Value::I16(n) => json!(n),
        Value::U16(n) => json!(n),
        Value::I32(n) => json!(n),
        Value::U32(n) => json!(n),
        Value::I64(n) => json!(n),
        Value::U64(n) => json!(n),
        Value::F64(n) => json!(n),
        Value::Str(s) => json!(s.as_str()),
        Value::Signature(s) => json!(s.as_str()),
        Value::ObjectPath(p) => json!(p.as_str()),
        Value::Value(inner) => decode(inner),
        Value::Array(a) => {
            let items: Vec<Json> = a.iter().map(decode).collect();
            if a.element_signature().as_str() == "y" {
                let bytes: Vec<u8> = items
                    .iter()
                    .filter_map(|j| j.as_u64().and_then(|n| u8::try_from(n).ok()))
                    .collect();
                json!({ "hex": hex(&bytes) })
            } else {
                Json::Array(items)
            }
        }
        Value::Dict(d) => {
            let mut map = serde_json::Map::new();
            for (k, item) in d.iter() {
                let key = match decode(k) {
                    Json::String(s) => s,
                    other => other.to_string(),
                };
                map.insert(key, decode(item));
            }
            Json::Object(map)
        }
        Value::Structure(s) => Json::Array(s.fields().iter().map(decode).collect()),
        Value::Fd(_) => json!("<fd>"),
    }
}

fn body_fields(msg: &zbus::Message) -> Result<Vec<Json>, String> {
    let body = msg.body();
    let Some(sig) = body
        .signature()
        .map(|s| s.as_str().to_owned())
        .filter(|s| !s.is_empty())
    else {
        return Ok(Vec::new());
    };
    let s: Structure = body.deserialize().map_err(|e| e.to_string())?;
    let fields: Vec<Json> = s.fields().iter().map(decode).collect();
    // zbus reads a body that is one struct as that struct's fields; on the wire
    // it is a single argument (a reply such as `Stat`'s entry).
    let one_struct = sig.starts_with('(') && split_types(&sig).is_ok_and(|t| t.len() == 1);
    Ok(if one_struct {
        vec![Json::Array(fields)]
    } else {
        fields
    })
}

// ---- matching ------------------------------------------------------------

/// Byte arrays may be written as `{"bytes": ...}`; both sides compare as hex.
fn normalise(v: &Json) -> Json {
    if v.as_object()
        .is_some_and(|o| o.len() == 1 && (o.contains_key("bytes") || o.contains_key("hex")))
    {
        if let Ok(b) = bytes_of(v) {
            return json!({ "hex": hex(&b) });
        }
    }
    v.clone()
}

type Bindings = HashMap<String, Json>;

fn matches(expected: &Json, actual: &Json, binds: &mut Bindings) -> Result<(), String> {
    let expected = normalise(expected);
    if let Some(s) = expected.as_str() {
        if s == "*" {
            return Ok(());
        }
        if let Some(name) = s.strip_prefix('$') {
            return match binds.get(name) {
                Some(bound) if bound == actual => Ok(()),
                Some(bound) => Err(format!("${name} was {bound}, now {actual}")),
                None => {
                    binds.insert(name.to_owned(), actual.clone());
                    Ok(())
                }
            };
        }
    }
    match (&expected, actual) {
        (Json::Array(e), Json::Array(a)) => match_arrays(e, a, binds),
        (Json::Object(e), Json::Object(a)) => {
            for (k, ev) in e {
                let av = a
                    .get(k)
                    .ok_or_else(|| format!("missing key \"{k}\" in {actual}"))?;
                matches(ev, av, binds).map_err(|m| format!("{k}: {m}"))?;
            }
            Ok(())
        }
        (e, a) if e == a => Ok(()),
        (e, a) => Err(format!("expected {e}, got {a}")),
    }
}

fn match_arrays(e: &[Json], a: &[Json], binds: &mut Bindings) -> Result<(), String> {
    let (items, prefix) = match e.split_last() {
        Some((last, rest)) if last.as_str() == Some("...") => (rest, true),
        _ => (e, false),
    };
    if a.len() < items.len() || (!prefix && a.len() != items.len()) {
        return Err(format!("expected {} elements, got {}", e.len(), a.len()));
    }
    for (i, (ev, av)) in items.iter().zip(a).enumerate() {
        matches(ev, av, binds).map_err(|m| format!("[{i}] {m}"))?;
    }
    Ok(())
}

/// Replaces `"$name"` strings by their bindings.
fn substitute(v: &Json, binds: &Bindings) -> Result<Json, String> {
    match v {
        Json::String(s) if s.starts_with('$') => binds
            .get(&s[1..])
            .cloned()
            .ok_or_else(|| format!("{s} is not bound yet")),
        Json::Array(a) => a
            .iter()
            .map(|x| substitute(x, binds))
            .collect::<Result<Vec<_>, _>>()
            .map(Json::Array),
        Json::Object(o) => o
            .iter()
            .map(|(k, x)| substitute(x, binds).map(|y| (k.clone(), y)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Json::Object),
        other => Ok(other.clone()),
    }
}

// ---- running -------------------------------------------------------------

#[derive(Default)]
struct Inbox {
    queue: Mutex<VecDeque<(String, Vec<Json>)>>,
    arrived: Notify,
}

impl Inbox {
    fn take(&self, member: &str) -> Option<Vec<Json>> {
        let mut q = self
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let at = q.iter().position(|(m, _)| m == member)?;
        q.remove(at).map(|(_, args)| args)
    }

    fn push(&self, member: String, args: Vec<Json>) {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back((member, args));
        self.arrived.notify_waiters();
    }
}

fn collect_signals(conn: &Connection, inbox: Arc<Inbox>) {
    let mut stream = zbus::MessageStream::from(conn.zbus());
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let header = msg.header();
            if header.primary().msg_type() != MessageType::Signal {
                continue;
            }
            let Some(member) = header.member().map(|m| m.as_str().to_owned()) else {
                continue;
            };
            if let Ok(args) = body_fields(&msg) {
                inbox.push(member, args);
            }
        }
    });
}

fn error_name(e: BridgeError) -> String {
    match e {
        BridgeError::Remote { name, .. } => name,
        BridgeError::InvalidArgs(_) => "org.freedesktop.DBus.Error.InvalidArgs".to_owned(),
        BridgeError::UnknownMethod(_) => "org.freedesktop.DBus.Error.UnknownMethod".to_owned(),
        other => format!("transport failure: {other}"),
    }
}

async fn call_step(
    conn: &Connection,
    (call, sig, args, outcome): (&str, &str, &[Json], &Outcome),
    binds: &mut Bindings,
) -> Result<(), String> {
    let types = split_types(sig)?;
    if types.len() != args.len() {
        return Err(format!("{} arguments for signature \"{sig}\"", args.len()));
    }
    let mut builder = StructureBuilder::new();
    for (t, a) in types.iter().zip(args) {
        builder = builder.append_field(encode(t, &substitute(a, binds)?)?);
    }
    let body = builder.build();
    let sent = conn
        .zbus()
        .call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), call, &body);
    let reply = tokio::time::timeout(STEP_TIMEOUT, sent)
        .await
        .map_err(|_| format!("{call}: no reply"))?;
    match (reply, outcome) {
        (Ok(msg), Outcome::Reply(expected)) => matches(
            &Json::Array(expected.clone()),
            &Json::Array(body_fields(&msg)?),
            binds,
        )
        .map_err(|m| format!("{call} reply: {m}")),
        (Ok(_), Outcome::Error(want)) => Err(format!("{call}: expected {want}, got a reply")),
        (Err(e), Outcome::Error(want)) => {
            let got = error_name(BridgeError::from(e));
            if &got == want {
                Ok(())
            } else {
                Err(format!("{call}: expected {want}, got {got}"))
            }
        }
        (Err(e), Outcome::Reply(_)) => Err(format!(
            "{call}: unexpected error {}",
            error_name(BridgeError::from(e))
        )),
    }
}

async fn signal_step(
    inbox: &Inbox,
    signal: &str,
    expected: &[Json],
    binds: &mut Bindings,
) -> Result<(), String> {
    let wait = async {
        loop {
            let notified = inbox.arrived.notified();
            tokio::pin!(notified);
            // Arm before looking, so that a signal pushed in between is not missed.
            notified.as_mut().enable();
            if let Some(args) = inbox.take(signal) {
                return args;
            }
            notified.await;
        }
    };
    let args = tokio::time::timeout(STEP_TIMEOUT, wait)
        .await
        .map_err(|_| format!("signal {signal} never arrived"))?;
    matches(&Json::Array(expected.to_vec()), &Json::Array(args), binds)
        .map_err(|m| format!("signal {signal}: {m}"))
}

/// Replays `seq` on a fresh fake bridge with the account `account:1`.
pub async fn replay(seq: &Sequence) -> Result<(), String> {
    let fake = FakeBridge::new();
    fake.set_consent(seq.consent).await;
    fake.add_account(1, "fake", "Fake 1", "fake.example").await;
    for (path, content) in &seq.files {
        fake.put_file(ACCOUNT, path.as_bytes(), content.as_bytes());
    }
    let conn = fake.connect().await.map_err(|e| e.to_string())?;
    let inbox = Arc::new(Inbox::default());
    collect_signals(&conn, inbox.clone());
    let mut binds = Bindings::new();
    for (i, step) in seq.steps.iter().enumerate() {
        let done = match step {
            Step::Call {
                call,
                sig,
                args,
                outcome,
            } => call_step(&conn, (call, sig, args, outcome), &mut binds).await,
            Step::Signal { signal, args } => signal_step(&inbox, signal, args, &mut binds).await,
        };
        done.map_err(|e| format!("step {}: {e}", i + 1))?;
    }
    Ok(())
}

/// Replays every `*.json` file of `dir` (the contract directory); returns the
/// names of the sequences that ran.
pub async fn replay_dir(dir: &Path) -> Result<Vec<String>, String> {
    let sources = tokio::task::spawn_blocking({
        let dir = dir.to_owned();
        move || read_sequences(&dir)
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut ran = Vec::new();
    for (name, source) in sources {
        let seq = parse(&source).map_err(|e| format!("{name}: {e}"))?;
        replay(&seq)
            .await
            .map_err(|e| format!("{name}: {} ({e})", seq.description))?;
        ran.push(name);
    }
    Ok(ran)
}

/// The `*.json` files of `dir` in name order, as (file name, contents).
fn read_sequences(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|file| {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let source = std::fs::read_to_string(&file).map_err(|e| format!("{name}: {e}"))?;
            Ok((name, source))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binds() -> Bindings {
        Bindings::new()
    }

    #[test]
    fn signature_splitting() {
        assert_eq!(split_types("saya{sv}").unwrap(), vec!["s", "ay", "a{sv}"]);
        assert_eq!(split_types("a(sssa{sv})").unwrap(), vec!["a(sssa{sv})"]);
        assert_eq!(split_types("").unwrap(), Vec::<&str>::new());
        assert!(split_types("(s").is_err());
        assert!(split_types("a").is_err());
    }

    #[test]
    fn encoding_by_signature() {
        assert_eq!(encode("u", &json!(7)).unwrap(), Value::U32(7));
        assert_eq!(encode("x", &json!(-7)).unwrap(), Value::I64(-7));
        assert!(encode("u", &json!(-1)).is_err());
        assert!(encode("y", &json!(256)).is_err());
        assert!(encode("b", &json!("x")).is_err());
        assert_eq!(
            encode("ay", &json!({"bytes": "ab"})).unwrap(),
            Value::from(vec![b'a', b'b'])
        );
        assert_eq!(
            encode("ay", &json!({"hex": "00ff"})).unwrap(),
            Value::from(vec![0u8, 255])
        );
        assert!(encode("ay", &json!({"hex": "0"})).is_err());
        assert!(encode("ay", &json!("x")).is_err());
        let d = encode("a{sv}", &json!({"n": 3, "b": true, "s": "x"})).unwrap();
        assert_eq!(decode(&d), json!({"n": 3, "b": true, "s": "x"}));
        assert!(encode("a{sv}", &json!({"l": [1]})).is_err());
        assert!(encode("a{ss}", &json!({})).is_err());
        let s = encode("(su)", &json!(["a", 1])).unwrap();
        assert_eq!(decode(&s), json!(["a", 1]));
        assert!(encode("(su)", &json!(["a"])).is_err());
        assert!(encode("h", &json!(1)).is_err());
        let a = encode("as", &json!(["x", "y"])).unwrap();
        assert_eq!(decode(&a), json!(["x", "y"]));
    }

    #[test]
    fn matching_rules() {
        let mut b = binds();
        assert!(matches(&json!("*"), &json!({"x": 1}), &mut b).is_ok());
        assert!(matches(&json!("$h"), &json!(5), &mut b).is_ok());
        assert!(matches(&json!("$h"), &json!(5), &mut b).is_ok());
        assert!(matches(&json!("$h"), &json!(6), &mut b).is_err());
        assert!(matches(&json!({"a": 1}), &json!({"a": 1, "b": 2}), &mut b).is_ok());
        assert!(matches(&json!({"a": 1, "c": 1}), &json!({"a": 1}), &mut b).is_err());
        assert!(matches(&json!([1, "..."]), &json!([1, 2, 3]), &mut b).is_ok());
        assert!(matches(&json!([1, 2, "..."]), &json!([1]), &mut b).is_err());
        assert!(matches(&json!([1]), &json!([1, 2]), &mut b).is_err());
        assert!(matches(&json!({"bytes": "ab"}), &json!({"hex": "6162"}), &mut b).is_ok());
        assert!(matches(&json!({"bytes": "ab"}), &json!({"hex": "6163"}), &mut b).is_err());
        assert!(matches(&json!(1), &json!(2), &mut b).is_err());
        assert!(matches(&json!("a"), &json!("a"), &mut b).is_ok());
    }

    #[test]
    fn substitution() {
        let mut b = binds();
        b.insert("h".into(), json!(9));
        assert_eq!(
            substitute(&json!(["$h", {"k": "$h"}, "x"]), &b).unwrap(),
            json!([9, {"k": 9}, "x"])
        );
        assert!(substitute(&json!("$nope"), &b).is_err());
    }

    #[test]
    fn parsing() {
        let seq = parse(
            r#"{"description":"d","consent":"unknown","files":{"a":"b"},"steps":[
                {"call":"X","sig":"","args":[],"reply":[]},
                {"call":"Y","sig":"","args":[],"error":"E"},
                {"signal":"S","args":["*"]}]}"#,
        )
        .unwrap();
        assert_eq!(seq.consent, Consent::Unknown);
        assert_eq!(seq.files, vec![("a".to_owned(), "b".to_owned())]);
        assert_eq!(seq.steps.len(), 3);
        assert!(parse(r#"{"description":"d","consent":"maybe","steps":[]}"#).is_err());
        assert!(
            parse(r#"{"description":"d","consent":"denied","steps":[{"call":"X","sig":"","args":[]}]}"#)
                .is_err()
        );
        assert!(parse("not json").is_err());
    }

    #[tokio::test]
    async fn inbox_hands_out_in_order() {
        let inbox = Inbox::default();
        inbox.push("A".into(), vec![json!(1)]);
        inbox.push("B".into(), vec![json!(2)]);
        inbox.push("A".into(), vec![json!(3)]);
        assert_eq!(inbox.take("A"), Some(vec![json!(1)]));
        assert_eq!(inbox.take("A"), Some(vec![json!(3)]));
        assert_eq!(inbox.take("A"), None);
        assert_eq!(inbox.take("B"), Some(vec![json!(2)]));
    }

    #[tokio::test]
    async fn a_failing_expectation_is_reported() {
        let seq = parse(
            r#"{"description":"d","consent":"granted","steps":[
                {"call":"Hello","sig":"us","args":[1,"t"],"reply":[2,"*","*"]}]}"#,
        )
        .unwrap();
        let err = replay(&seq).await.unwrap_err();
        assert!(err.contains("step 1"), "{err}");
    }
}
