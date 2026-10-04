// SPDX-License-Identifier: LGPL-2.1-or-later
//! `MediaSource` (PRV-8, PRV-9): plays a (remote) media file with
//! QtMultimedia without downloading it first.
//!
//! QML's `MediaPlayer` cannot take a `QIODevice`, so this type owns a
//! `QMediaPlayer` and exposes it as the `mediaObject` property, which is what
//! `VideoOutput { source: mediaSource }` looks for. The player reads through
//! a `QIODevice` subclass whose `readData` calls back into Rust, where a
//! [`ReadAhead`] keeps a 2 MiB window over the bridge read handle.
//!
//! PRV-9: when the device's GStreamer backend cannot play from a `QIODevice`
//! (or cannot seek in it), the player reports a resource or format error.
//! `MediaSource` then falls back to download-then-play with progress
//! (`downloading`, `downloadProgress`), also selectable up front with
//! `preferDownload`. `mode` tells which path is in use.

use super::{parse_uri, qstr, run_then};
use crate::runtime::{core, handle};
use cpp::cpp;
use lautta_core::app_viewers::ReadAhead;
use lautta_core::org::recents::RecentKind;
use lautta_core::provider::ProgressSink;
use qmetaobject::prelude::*;
use qmetaobject::{qml_register_type, QPointer};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub fn register() {
    qml_register_type::<MediaSource>(&crate::qml_uri(), 1, 0, &crate::cstr("MediaSource"));
}

/// How often the player's position and state are read.
const POLL: Duration = Duration::from_millis(250);

cpp! {{
    #include <QtMultimedia/QMediaPlayer>
    #include <QtMultimedia/QMediaContent>
    #include <QtCore/QIODevice>
    #include <QtCore/QUrl>
    #include <QtCore/QVariant>

    // Random-access device over a Rust `ReadAhead` (PRV-8). Reads block the
    // calling (player) thread until the provider answers.
    class LauttaMediaDevice : public QIODevice {
    public:
        LauttaMediaDevice(void *ctx, qint64 size) : m_ctx(ctx), m_size(size) { open(QIODevice::ReadOnly); }
        ~LauttaMediaDevice() override
        {
            void *ctx = m_ctx;
            rust!(Lautta_media_release [ctx: *mut c_void as "void*"] { release_reader(ctx); });
        }
        bool isSequential() const override { return false; }
        qint64 size() const override { return m_size; }

    protected:
        qint64 readData(char *data, qint64 maxlen) override
        {
            const qint64 off = pos();
            if (off >= m_size)
                return 0;
            const qint64 want = qMin(maxlen, m_size - off);
            void *ctx = m_ctx;
            return rust!(Lautta_media_read [ctx: *mut c_void as "void*", off: i64 as "qint64",
                                            data: *mut u8 as "char*", want: i64 as "qint64"]
                                           -> i64 as "qint64" { read_into(ctx, off, data, want) });
        }
        qint64 writeData(const char *, qint64) override { return -1; }

    private:
        void *m_ctx;
        qint64 m_size;
    };

    struct LauttaMedia {
        QMediaPlayer *player = nullptr;
        LauttaMediaDevice *device = nullptr;
    };

    static LauttaMedia *lauttaMediaNew()
    {
        auto *m = new LauttaMedia;
        m->player = new QMediaPlayer(nullptr, QMediaPlayer::StreamPlayback);
        return m;
    }

    static void lauttaMediaDropDevice(LauttaMedia *m)
    {
        if (m->device) {
            m->player->stop();
            m->player->setMedia(QMediaContent());
            delete m->device;
            m->device = nullptr;
        }
    }

    static void lauttaMediaStream(LauttaMedia *m, void *ctx, qint64 size)
    {
        lauttaMediaDropDevice(m);
        m->device = new LauttaMediaDevice(ctx, size);
        m->player->setMedia(QMediaContent(), m->device);
    }

    static void lauttaMediaFile(LauttaMedia *m, const QString &path)
    {
        lauttaMediaDropDevice(m);
        m->player->setMedia(QMediaContent(QUrl::fromLocalFile(path)));
    }

    static void lauttaMediaFree(LauttaMedia *m)
    {
        m->player->stop();
        m->player->setMedia(QMediaContent());
        delete m->player;
        delete m->device;
        delete m;
    }
}}

/// The `Arc<ReadAhead>` handed to the device, as a raw pointer.
fn reader_ptr(r: Arc<ReadAhead>) -> *mut c_void {
    Arc::into_raw(r) as *mut c_void
}

fn release_reader(ctx: *mut c_void) {
    // SAFETY: `ctx` came from `reader_ptr` and the device releases it once.
    drop(unsafe { Arc::from_raw(ctx as *const ReadAhead) });
}

/// Called on the player's thread: fills `buf` with up to `want` bytes at
/// `off`; -1 on a read error.
fn read_into(ctx: *mut c_void, off: i64, buf: *mut u8, want: i64) -> i64 {
    // SAFETY: the device keeps the reader alive while it can call us.
    let reader = unsafe { &*(ctx as *const ReadAhead) };
    let (Ok(off), Ok(want)) = (u64::try_from(off), usize::try_from(want)) else {
        return -1;
    };
    match handle().block_on(reader.read(off, want)) {
        Ok(data) => {
            let n = data.len().min(want);
            // SAFETY: Qt gives a buffer of at least `want` bytes.
            unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), buf, n) };
            i64::try_from(n).unwrap_or(-1)
        }
        Err(e) => {
            log::debug!("media read failed: {e}");
            -1
        }
    }
}

/// QMediaPlayer::Error to the app's error kinds.
pub fn error_kind(code: i32) -> &'static str {
    match code {
        0 => "",
        1 | 2 | 5 => "Unsupported",
        3 => "ConnectionLost",
        4 => "PermissionDenied",
        _ => "Internal",
    }
}

/// QMediaPlayer::State to the engineering names.
pub fn state_name(code: i32) -> &'static str {
    match code {
        1 => "playing",
        2 => "paused",
        _ => "stopped",
    }
}

/// QMediaPlayer errors a download can cure: the device source did not work.
fn fallback_applies(code: i32) -> bool {
    matches!(code, 1 | 2)
}

#[derive(Default)]
struct Progress {
    done: AtomicU64,
    total: AtomicU64,
}

#[derive(QObject, Default)]
pub struct MediaSource {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    /// The `QMediaPlayer`, for `VideoOutput { source: … }`.
    mediaObject: qt_property!(QVariant; READ getMediaObject NOTIFY mediaObjectChanged),
    mediaObjectChanged: qt_signal!(),
    /// `stopped`, `playing` or `paused`.
    state: qt_property!(QString; NOTIFY changed),
    playing: qt_property!(bool; NOTIFY changed),
    /// Milliseconds.
    position: qt_property!(f64; NOTIFY changed),
    duration: qt_property!(f64; NOTIFY changed),
    /// The source is being opened (stat, first read).
    loading: qt_property!(bool; NOTIFY changed),
    /// `stream` (QIODevice, PRV-8) or `download` (PRV-9 fallback).
    mode: qt_property!(QString; NOTIFY changed),
    /// Plays after a full download instead of streaming (PRV-9).
    preferDownload: qt_property!(bool; WRITE setPreferDownload NOTIFY changed),
    downloading: qt_property!(bool; NOTIFY changed),
    /// 0..1 while downloading, -1 when the size is unknown.
    downloadProgress: qt_property!(f64; NOTIFY changed),
    errorKind: qt_property!(QString; NOTIFY changed),
    errorMessage: qt_property!(QString; NOTIFY changed),
    changed: qt_signal!(),

    play: qt_method!(fn(&mut self)),
    pause: qt_method!(fn(&mut self)),
    stop: qt_method!(fn(&mut self)),
    seek: qt_method!(fn(&mut self, ms: f64)),

    media: usize,
    want_play: bool,
    fell_back: bool,
    opened: bool,
    generation: u64,
    progress: Arc<Progress>,
}

impl MediaSource {
    fn player_ptr(&self) -> *mut c_void {
        self.media as *mut c_void
    }

    fn getMediaObject(&self) -> QVariant {
        let m = self.player_ptr();
        if m.is_null() {
            return QVariant::default();
        }
        cpp!(unsafe [m as "LauttaMedia*"] -> QVariant as "QVariant" {
            return QVariant::fromValue(static_cast<QObject *>(m->player));
        })
    }

    fn player_info(&self, what: i32) -> i64 {
        let m = self.player_ptr();
        if m.is_null() {
            return 0;
        }
        cpp!(unsafe [m as "LauttaMedia*", what as "int"] -> i64 as "qint64" {
            switch (what) {
            case 0: return m->player->position();
            case 1: return m->player->duration();
            case 2: return m->player->state();
            default: return m->player->error();
            }
        })
    }

    fn player_error_text(&self) -> QString {
        let m = self.player_ptr();
        cpp!(unsafe [m as "LauttaMedia*"] -> QString as "QString" { return m->player->errorString(); })
    }

    fn ensure_player(&mut self) {
        if self.media == 0 {
            let m = cpp!(unsafe [] -> *mut c_void as "LauttaMedia*" { return lauttaMediaNew(); });
            self.media = m as usize;
            self.mediaObjectChanged();
            self.schedule_poll();
        }
    }

    fn schedule_poll(&self) {
        let ptr = QPointer::from(self);
        qmetaobject::single_shot(POLL, move || {
            if let Some(me) = ptr.as_pinned() {
                let mut me = me.borrow_mut();
                me.poll();
                me.schedule_poll();
            }
        });
    }

    /// Reads the player's state into the properties; signals only changes.
    fn poll(&mut self) {
        if self.media == 0 {
            return;
        }
        let position = self.player_info(0) as f64;
        let duration = self.player_info(1) as f64;
        let state = state_name(i32::try_from(self.player_info(2)).unwrap_or(0));
        let code = i32::try_from(self.player_info(3)).unwrap_or(0);
        let mut changed = position != self.position || duration != self.duration;
        changed |= state != self.state.to_string();
        self.position = position;
        self.duration = duration;
        self.state = qstr(state);
        self.playing = state == "playing";
        if code != 0 {
            changed |= self.player_failed(code);
        }
        if self.downloading {
            changed |= self.update_progress();
        }
        if changed {
            self.changed();
        }
    }

    fn update_progress(&mut self) -> bool {
        let done = self.progress.done.load(Ordering::Relaxed) as f64;
        let total = self.progress.total.load(Ordering::Relaxed) as f64;
        let value = if total > 0.0 {
            (done / total).min(1.0)
        } else {
            -1.0
        };
        let changed = value != self.downloadProgress;
        self.downloadProgress = value;
        changed
    }

    /// The player reported an error: fall back to a download (PRV-9) or
    /// publish it.
    fn player_failed(&mut self, code: i32) -> bool {
        if self.mode.to_string() == "stream" && !self.fell_back && fallback_applies(code) {
            self.fell_back = true;
            self.start_download();
            return true;
        }
        let kind = qstr(error_kind(code));
        if kind == self.errorKind {
            return false;
        }
        self.errorKind = kind;
        self.errorMessage = self.player_error_text();
        true
    }

    fn setUri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.uri = value;
        self.uriChanged();
        self.generation += 1;
        self.fell_back = false;
        self.opened = false;
        self.errorKind = QString::default();
        self.errorMessage = QString::default();
        self.downloading = false;
        self.downloadProgress = 0.0;
        if parse_uri(&self.uri).is_none() {
            self.changed();
            return;
        }
        self.ensure_player();
        self.open();
    }

    fn setPreferDownload(&mut self, on: bool) {
        if self.preferDownload == on {
            return;
        }
        self.preferDownload = on;
        if on && self.opened && self.mode.to_string() == "stream" {
            self.fell_back = true;
            self.start_download();
        }
        self.changed();
    }

    fn open(&mut self) {
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return;
        };
        if self.preferDownload {
            self.start_download();
            return;
        }
        self.loading = true;
        self.mode = qstr("stream");
        self.changed();
        let gen = self.generation;
        run_then(
            self,
            async move {
                let reader = ReadAhead::new(core.open_reader(&uri).await?)?;
                core.note_viewed(&uri, RecentKind::Previewed);
                Ok(Arc::new(reader))
            },
            move |me, res| {
                if me.generation != gen {
                    return;
                }
                me.loading = false;
                match res {
                    Ok(reader) => me.attach(reader),
                    Err(e) => {
                        me.errorKind = qstr(e.kind.name());
                        me.errorMessage = qstr(&e.message);
                    }
                }
                me.changed();
            },
        );
    }

    fn attach(&mut self, reader: Arc<ReadAhead>) {
        let size = i64::try_from(reader.size()).unwrap_or(i64::MAX);
        let ctx = reader_ptr(reader);
        let m = self.player_ptr();
        cpp!(unsafe [m as "LauttaMedia*", ctx as "void*", size as "qint64"] {
            lauttaMediaStream(m, ctx, size);
        });
        self.opened = true;
        if self.want_play {
            self.play();
        }
    }

    /// PRV-9: copy the file into the private cache, then play that.
    fn start_download(&mut self) {
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            return;
        };
        self.mode = qstr("download");
        self.downloading = true;
        self.loading = false;
        self.downloadProgress = -1.0;
        self.errorKind = QString::default();
        self.progress = Arc::new(Progress::default());
        let progress = Arc::clone(&self.progress);
        let sink: ProgressSink = Arc::new(move |done, total| {
            progress.done.store(done, Ordering::Relaxed);
            progress.total.store(total.unwrap_or(0), Ordering::Relaxed);
        });
        let gen = self.generation;
        run_then(
            self,
            async move {
                core.note_viewed(&uri, RecentKind::Previewed);
                core.download_for_playback(&uri, sink).await
            },
            move |me, res| {
                if me.generation != gen {
                    return;
                }
                me.downloading = false;
                match res {
                    Ok(path) => me.play_file(&path.to_string_lossy()),
                    Err(e) => {
                        me.errorKind = qstr(e.kind.name());
                        me.errorMessage = qstr(&e.message);
                    }
                }
                me.changed();
            },
        );
    }

    fn play_file(&mut self, path: &str) {
        let m = self.player_ptr();
        let path = QString::from(path);
        cpp!(unsafe [m as "LauttaMedia*", path as "QString"] { lauttaMediaFile(m, path); });
        self.opened = true;
        self.errorKind = QString::default();
        self.downloadProgress = 1.0;
        if self.want_play {
            self.play();
        }
    }

    fn play(&mut self) {
        self.want_play = true;
        if self.media != 0 && self.opened {
            let m = self.player_ptr();
            cpp!(unsafe [m as "LauttaMedia*"] { m->player->play(); });
        }
    }

    fn pause(&mut self) {
        self.want_play = false;
        if self.media != 0 {
            let m = self.player_ptr();
            cpp!(unsafe [m as "LauttaMedia*"] { m->player->pause(); });
        }
    }

    fn stop(&mut self) {
        self.want_play = false;
        if self.media != 0 {
            let m = self.player_ptr();
            cpp!(unsafe [m as "LauttaMedia*"] { m->player->stop(); });
        }
    }

    fn seek(&mut self, ms: f64) {
        if self.media != 0 {
            let m = self.player_ptr();
            let ms = ms.max(0.0) as i64;
            cpp!(unsafe [m as "LauttaMedia*", ms as "qint64"] { m->player->setPosition(ms); });
        }
    }
}

impl Drop for MediaSource {
    fn drop(&mut self) {
        if self.media != 0 {
            let m = self.player_ptr();
            // The device (and with it the reader) goes with the player.
            cpp!(unsafe [m as "LauttaMedia*"] { lauttaMediaFree(m); });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_map_to_app_kinds() {
        assert_eq!(error_kind(0), "");
        assert_eq!(error_kind(1), "Unsupported");
        assert_eq!(error_kind(2), "Unsupported");
        assert_eq!(error_kind(3), "ConnectionLost");
        assert_eq!(error_kind(4), "PermissionDenied");
        assert_eq!(error_kind(5), "Unsupported");
        assert_eq!(error_kind(9), "Internal");
    }

    #[test]
    fn states_have_names() {
        assert_eq!(state_name(0), "stopped");
        assert_eq!(state_name(1), "playing");
        assert_eq!(state_name(2), "paused");
        assert_eq!(state_name(7), "stopped");
    }

    #[test]
    fn only_device_failures_fall_back_to_a_download() {
        assert!(fallback_applies(1) && fallback_applies(2));
        assert!(!fallback_applies(3) && !fallback_applies(4) && !fallback_applies(5));
    }

    #[test]
    fn the_device_reads_through_the_reader() {
        use lautta_core::provider::memory::MemoryProvider;
        use lautta_core::provider::{Lane, Provider};
        let rt = crate::runtime::handle();
        let mem = MemoryProvider::default();
        mem.add_file("a.bin", &(0..=255u8).cycle().take(10_000).collect::<Vec<_>>(), 0);
        let path = lautta_core::vpath::VPath::parse(b"a.bin").unwrap();
        let reader = rt.block_on(async {
            let h = mem.open_read(&path, Lane::Interactive).await.unwrap();
            ReadAhead::new(Arc::from(h)).unwrap()
        });
        let ctx = reader_ptr(Arc::new(reader));
        let mut buf = vec![0u8; 100];
        assert_eq!(read_into(ctx, 250, buf.as_mut_ptr(), 100), 100);
        assert_eq!(buf[0], 250);
        assert_eq!(buf[10], 4);
        assert_eq!(read_into(ctx, 9_990, buf.as_mut_ptr(), 100), 10, "cut at the end");
        assert_eq!(read_into(ctx, 10_000, buf.as_mut_ptr(), 100), 0);
        assert_eq!(read_into(ctx, -1, buf.as_mut_ptr(), 100), -1);
        release_reader(ctx);
    }
}
