// SPDX-License-Identifier: LGPL-2.1-or-later
//! Error presentation (SPEC §20): one table of messages and actions for all
//! providers (RS-8), plus the humanised sizes, rates and times used across
//! the UI (§15.4). Messages name the location or item involved, never
//! secrets (SEC-6).

use crate::error::{Error, ErrorKind};

/// Buttons a message can offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiAction {
    /// NVB-5: open the account's sign-in.
    UpdateSignIn,
    /// NVB-5: review a changed server identity.
    ReviewInSettings,
    AccountSettings,
    ChooseAnotherFolder,
    Retry,
    RetryNow,
    Details,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMessage {
    pub text: String,
    pub actions: Vec<UiAction>,
}

/// What the message is about. All parts are optional; wording falls back to
/// "this location" and "this item".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageContext {
    pub location: Option<String>,
    pub item: Option<String>,
    pub needed_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
}

impl MessageContext {
    pub fn location(name: &str) -> MessageContext {
        MessageContext {
            location: Some(name.to_owned()),
            ..MessageContext::default()
        }
    }

    pub fn item(name: &str) -> MessageContext {
        MessageContext {
            item: Some(name.to_owned()),
            ..MessageContext::default()
        }
    }

    pub fn with_location(mut self, name: &str) -> MessageContext {
        self.location = Some(name.to_owned());
        self
    }

    pub fn with_item(mut self, name: &str) -> MessageContext {
        self.item = Some(name.to_owned());
        self
    }

    pub fn with_space(mut self, needed: u64, free: u64) -> MessageContext {
        self.needed_bytes = Some(needed);
        self.free_bytes = Some(free);
        self
    }

    fn loc(&self) -> &str {
        self.location.as_deref().unwrap_or("this location")
    }

    fn it(&self) -> &str {
        self.item.as_deref().unwrap_or("this item")
    }
}

fn no_space(ctx: &MessageContext) -> String {
    let detail = match (ctx.needed_bytes, ctx.free_bytes) {
        (Some(n), Some(f)) => format!(" (needs {}, {} free)", format_size(n), format_size(f)),
        (Some(n), None) => format!(" (needs {})", format_size(n)),
        (None, Some(f)) => format!(" ({} free)", format_size(f)),
        (None, None) => String::new(),
    };
    format!("Not enough space on {}{detail}.", ctx.loc())
}

fn msg(text: String, actions: &[UiAction]) -> UiMessage {
    UiMessage {
        text,
        actions: actions.to_vec(),
    }
}

/// The text of a message for `kind`; the first group is the §20 table
/// verbatim, the rest follows its tone.
fn text_for(kind: ErrorKind, ctx: &MessageContext) -> String {
    let (loc, it) = (ctx.loc(), ctx.it());
    match kind {
        ErrorKind::AuthFailed => format!("The server didn't accept the sign-in for {loc}."),
        ErrorKind::ServerIdentityChanged => format!(
            "{loc} is presenting a different identity than before. This can mean it was \
             reinstalled \u{2014} or that someone is intercepting the connection."
        ),
        ErrorKind::SecurityPolicy => format!("{loc} doesn't meet the account's security settings."),
        ErrorKind::NoSpace => no_space(ctx),
        ErrorKind::PermissionDenied => format!("You don't have permission to change {it}."),
        ErrorKind::Locked => format!("{it} is open elsewhere."),
        ErrorKind::ConnectionLost => format!("Connection to {loc} was lost. Retrying\u{2026}"),
        ErrorKind::Unsupported => format!("{loc} can't set permissions."),
        ErrorKind::BridgeUnavailable => "Network locations are unavailable right now.".to_owned(),
        ErrorKind::Sandbox => "Lautta isn't allowed to read this file.".to_owned(),
        other => text_for_other(other, ctx),
    }
}

fn text_for_other(kind: ErrorKind, ctx: &MessageContext) -> String {
    let (loc, it) = (ctx.loc(), ctx.it());
    match kind {
        ErrorKind::NotFound => format!("{it} doesn't exist any more."),
        ErrorKind::AlreadyExists => format!("{it} already exists."),
        ErrorKind::InvalidArgument => "That isn't something Lautta can do.".to_owned(),
        ErrorKind::Canceled => "Canceled.".to_owned(),
        ErrorKind::TimedOut => format!("{loc} took too long to answer."),
        ErrorKind::NetworkUnreachable => format!("{loc} can't be reached. Check the network connection."),
        ErrorKind::ServerIdentityUnknown => format!("{loc} is a server Lautta hasn't seen before."),
        ErrorKind::ProtocolError => format!("{loc} sent an answer Lautta couldn't understand."),
        ErrorKind::NotADirectory => format!("{it} isn't a folder."),
        ErrorKind::IsADirectory => format!("{it} is a folder."),
        ErrorKind::DirectoryNotEmpty => format!("{it} isn't empty."),
        ErrorKind::InvalidName => format!("{loc} doesn't accept that name."),
        ErrorKind::ReadOnlyFilesystem => format!("{loc} is read-only."),
        ErrorKind::TooManyConnections => {
            format!("{loc} has too many connections open. Trying again shortly.")
        }
        ErrorKind::RateLimited => format!("{loc} asked Lautta to slow down. Trying again shortly."),
        ErrorKind::NotModified => format!("{it} hasn't changed."),
        ErrorKind::CrossesDevice => format!("{it} can't be moved to {loc} directly."),
        ErrorKind::Io => format!("Something went wrong while accessing {it}."),
        ErrorKind::Internal => "Something went wrong inside Lautta.".to_owned(),
        // The §20 group is handled by `text_for`; kept total for safety.
        _ => "Something went wrong.".to_owned(),
    }
}

fn actions_for(kind: ErrorKind) -> &'static [UiAction] {
    match kind {
        ErrorKind::AuthFailed => &[UiAction::UpdateSignIn],
        ErrorKind::ServerIdentityChanged | ErrorKind::ServerIdentityUnknown => &[UiAction::ReviewInSettings],
        ErrorKind::SecurityPolicy => &[UiAction::AccountSettings],
        ErrorKind::NoSpace => &[UiAction::ChooseAnotherFolder],
        ErrorKind::Locked
        | ErrorKind::BridgeUnavailable
        | ErrorKind::TimedOut
        | ErrorKind::NetworkUnreachable => &[UiAction::Retry],
        ErrorKind::ConnectionLost | ErrorKind::TooManyConnections | ErrorKind::RateLimited => {
            &[UiAction::RetryNow]
        }
        ErrorKind::ProtocolError | ErrorKind::Io | ErrorKind::Internal => &[UiAction::Details],
        _ => &[],
    }
}

/// The message and actions for `kind` (§20).
pub fn message_for(kind: ErrorKind, ctx: &MessageContext) -> UiMessage {
    msg(text_for(kind, ctx), actions_for(kind))
}

/// Like [`message_for`], and adds *Details* whenever the error carries a
/// protocol detail to show.
pub fn message_for_error(err: &Error, ctx: &MessageContext) -> UiMessage {
    let mut m = message_for(err.kind, ctx);
    if err.detail.is_some() && !m.actions.contains(&UiAction::Details) {
        m.actions.push(UiAction::Details);
    }
    m
}

const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];

/// SI sizes as Sailfish shows them: "512 B", "1.5 kB", "1.2 GB".
pub fn format_size(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = 0;
    while (value >= 1000.0 || (value * 10.0).round() >= 10_000.0) && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// "11.4 MB/s".
pub fn format_rate(bytes_per_second: u64) -> String {
    format!("{}/s", format_size(bytes_per_second))
}

/// "12 s", "4 min", "1 h 5 min", "2 d 3 h".
pub fn format_eta(seconds: u64) -> String {
    const MIN: u64 = 60;
    const HOUR: u64 = 3600;
    const DAY: u64 = 86_400;
    if seconds < MIN {
        return format!("{seconds} s");
    }
    if seconds < HOUR - MIN / 2 {
        return format!("{} min", (seconds + MIN / 2) / MIN);
    }
    if seconds < DAY - HOUR / 2 {
        let total_min = (seconds + MIN / 2) / MIN;
        let (h, m) = (total_min / 60, total_min % 60);
        return if m == 0 {
            format!("{h} h")
        } else {
            format!("{h} h {m} min")
        };
    }
    let total_h = (seconds + HOUR / 2) / HOUR;
    let (d, h) = (total_h / 24, total_h % 24);
    if h == 0 {
        format!("{d} d")
    } else {
        format!("{d} d {h} h")
    }
}

/// The progress line of a transfer (§15.4): "1.2 GB of 4.0 GB · 11.4 MB/s ·
/// 4 min". Unknown totals, rates and estimates are left out.
pub fn transfer_line(done: u64, total: Option<u64>, rate: Option<u64>, eta_seconds: Option<u64>) -> String {
    let mut parts = vec![match total {
        Some(t) => format!("{} of {}", format_size(done), format_size(t)),
        None => format_size(done),
    }];
    if let Some(r) = rate.filter(|r| *r > 0) {
        parts.push(format_rate(r));
        if let Some(eta) = eta_seconds {
            parts.push(format_eta(eta));
        }
    }
    parts.join(" \u{b7} ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_message() {
        let ctx = MessageContext::default();
        for kind in ErrorKind::ALL {
            let m = message_for(kind, &ctx);
            assert!(!m.text.trim().is_empty(), "{kind}");
            assert_ne!(
                m.text, "Something went wrong.",
                "{kind} fell through to the fallback"
            );
            assert!(
                m.text.ends_with('.') || m.text.ends_with('\u{2026}'),
                "{kind}: {}",
                m.text
            );
            let named = message_for(kind, &MessageContext::location("NAS").with_item("Report.docx"));
            assert!(!named.text.is_empty());
        }
    }

    #[test]
    fn spec_table_texts_are_exact() {
        let nas = MessageContext::location("NAS");
        assert_eq!(
            message_for(ErrorKind::AuthFailed, &nas),
            UiMessage {
                text: "The server didn't accept the sign-in for NAS.".into(),
                actions: vec![UiAction::UpdateSignIn]
            }
        );
        let m = message_for(ErrorKind::ServerIdentityChanged, &nas);
        assert_eq!(
            m.text,
            "NAS is presenting a different identity than before. This can mean it was \
             reinstalled \u{2014} or that someone is intercepting the connection."
        );
        assert_eq!(m.actions, [UiAction::ReviewInSettings]);
        let m = message_for(ErrorKind::SecurityPolicy, &nas);
        assert_eq!(m.text, "NAS doesn't meet the account's security settings.");
        assert_eq!(m.actions, [UiAction::AccountSettings]);
        let m = message_for(ErrorKind::PermissionDenied, &MessageContext::item("Photos"));
        assert_eq!(m.text, "You don't have permission to change Photos.");
        assert!(m.actions.is_empty());
        let m = message_for(ErrorKind::Locked, &MessageContext::item("Report.docx"));
        assert_eq!(m.text, "Report.docx is open elsewhere.");
        assert_eq!(m.actions, [UiAction::Retry]);
        let m = message_for(ErrorKind::ConnectionLost, &nas);
        assert_eq!(m.text, "Connection to NAS was lost. Retrying\u{2026}");
        assert_eq!(m.actions, [UiAction::RetryNow]);
        let m = message_for(ErrorKind::Unsupported, &nas);
        assert_eq!(m.text, "NAS can't set permissions.");
        assert!(m.actions.is_empty());
        let m = message_for(ErrorKind::BridgeUnavailable, &MessageContext::default());
        assert_eq!(m.text, "Network locations are unavailable right now.");
        assert_eq!(m.actions, [UiAction::Retry]);
        let m = message_for(ErrorKind::Sandbox, &MessageContext::default());
        assert_eq!(m.text, "Lautta isn't allowed to read this file.");
        assert!(m.actions.is_empty());
    }

    #[test]
    fn no_space_names_the_numbers() {
        let ctx = MessageContext::location("Destination").with_space(4_100_000_000, 1_200_000_000);
        let m = message_for(ErrorKind::NoSpace, &ctx);
        assert_eq!(
            m.text,
            "Not enough space on Destination (needs 4.1 GB, 1.2 GB free)."
        );
        assert_eq!(m.actions, [UiAction::ChooseAnotherFolder]);
        let partial = MessageContext {
            needed_bytes: Some(2_000_000),
            ..MessageContext::location("SD card")
        };
        assert_eq!(
            message_for(ErrorKind::NoSpace, &partial).text,
            "Not enough space on SD card (needs 2.0 MB)."
        );
        let free_only = MessageContext {
            free_bytes: Some(500),
            ..MessageContext::default()
        };
        assert_eq!(
            message_for(ErrorKind::NoSpace, &free_only).text,
            "Not enough space on this location (500 B free)."
        );
        assert_eq!(
            message_for(ErrorKind::NoSpace, &MessageContext::default()).text,
            "Not enough space on this location."
        );
    }

    #[test]
    fn details_action_follows_error_detail() {
        let plain = Error::new(ErrorKind::NotFound, "x");
        assert!(message_for_error(&plain, &MessageContext::default())
            .actions
            .is_empty());
        let detailed = Error::new(ErrorKind::NotFound, "x").with_detail("org.netvfs.Error.NotFound: path");
        assert_eq!(
            message_for_error(&detailed, &MessageContext::default()).actions,
            [UiAction::Details]
        );
        let internal = Error::new(ErrorKind::Internal, "x").with_detail("d");
        assert_eq!(
            message_for_error(&internal, &MessageContext::default()).actions,
            [UiAction::Details],
            "no duplicate"
        );
    }

    #[test]
    fn messages_never_echo_error_text() {
        let err = Error::new(ErrorKind::AuthFailed, "password hunter2 rejected");
        let m = message_for_error(&err, &MessageContext::location("NAS"));
        assert!(!m.text.contains("hunter2"));
    }

    #[test]
    fn sizes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1_000), "1.0 kB");
        assert_eq!(format_size(1_500), "1.5 kB");
        assert_eq!(format_size(999_949), "999.9 kB");
        assert_eq!(format_size(999_950), "1.0 MB");
        assert_eq!(format_size(1_200_000_000), "1.2 GB");
        assert_eq!(format_size(4_000_000_000), "4.0 GB");
        assert_eq!(format_size(11_400_000), "11.4 MB");
        assert_eq!(format_size(u64::MAX), "18446744.1 TB");
        assert_eq!(format_size(2_500_000_000_000), "2.5 TB");
    }

    #[test]
    fn rates_and_etas() {
        assert_eq!(format_rate(11_400_000), "11.4 MB/s");
        assert_eq!(format_rate(0), "0 B/s");
        assert_eq!(format_eta(0), "0 s");
        assert_eq!(format_eta(12), "12 s");
        assert_eq!(format_eta(59), "59 s");
        assert_eq!(format_eta(60), "1 min");
        assert_eq!(format_eta(219), "4 min");
        assert_eq!(format_eta(240), "4 min");
        assert_eq!(format_eta(3_569), "59 min");
        assert_eq!(format_eta(3_570), "1 h");
        assert_eq!(format_eta(3_900), "1 h 5 min");
        assert_eq!(format_eta(7_200), "2 h");
        assert_eq!(format_eta(86_399), "1 d");
        assert_eq!(format_eta(97_200), "1 d 3 h");
        assert_eq!(format_eta(172_800), "2 d");
    }

    #[test]
    fn transfer_lines() {
        assert_eq!(
            transfer_line(1_200_000_000, Some(4_000_000_000), Some(11_400_000), Some(240)),
            "1.2 GB of 4.0 GB \u{b7} 11.4 MB/s \u{b7} 4 min"
        );
        assert_eq!(transfer_line(5_000, None, None, None), "5.0 kB");
        assert_eq!(
            transfer_line(5_000, Some(10_000), Some(0), Some(5)),
            "5.0 kB of 10.0 kB"
        );
        assert_eq!(
            transfer_line(5_000, Some(10_000), Some(1_000), None),
            "5.0 kB of 10.0 kB \u{b7} 1.0 kB/s"
        );
    }
}
