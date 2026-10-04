// SPDX-License-Identifier: LGPL-2.1-or-later
//! Images (PRV-2, PRV-3, PRV-4): `ImageListModel` (the folder's images, for
//! swiping) and the two QML image providers for remote files,
//! `image://lautta-thumb/<uri>?size=N` and `image://lautta-file/<uri>`.
//!
//! Local files never come here for thumbnails (PRV-1: `Nemo.Thumbnailer`);
//! the full-image provider serves any location, which is what the viewer
//! uses for remote files. The providers are asynchronous
//! (`QQuickAsyncImageProvider`): the request returns at once, the tokio
//! runtime fetches, a worker decodes, and the response is finished from
//! there.
//!
//! The engine must be given the providers once: see
//! [`register_image_providers`].

use super::{parse_uri, qstr, run_then};
use crate::runtime::{core, handle};
use cpp::cpp;
use lautta_core::app_viewers::{parse_image_uri, parse_thumb_id, ImageItem};
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::Mutex;

pub fn register() {
    qml_register_type::<ImageListModel>(&crate::qml_uri(), 1, 0, &crate::cstr("ImageListModel"));
}

const ROLE_URI: i32 = 0x100;
const ROLE_NAME: i32 = 0x101;
const ROLE_SIZE: i32 = 0x102;

/// The images of a folder in the folder's own sort order.
#[derive(QObject, Default)]
pub struct ImageListModel {
    base: qt_base_class!(trait QAbstractListModel),
    folderUri: qt_property!(QString; WRITE setFolderUri NOTIFY folderUriChanged),
    folderUriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    count: qt_property!(i32; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),
    /// Row of an image URI, -1 when the folder has no such image.
    indexOf: qt_method!(fn(&self, uri: QString) -> i32),
    /// Looks at the folder again (after a delete).
    reload: qt_method!(fn(&mut self)),
    items: Vec<ImageItem>,
    generation: u64,
}

impl ImageListModel {
    fn setFolderUri(&mut self, value: QString) {
        if self.folderUri == value {
            return;
        }
        self.folderUri = value;
        self.folderUriChanged();
        self.reload();
    }

    fn reload(&mut self) {
        self.generation += 1;
        let gen = self.generation;
        let (Some(core), Some(folder)) = (core(), parse_uri(&self.folderUri)) else {
            return;
        };
        self.loading = true;
        self.errorKind = QString::default();
        self.stateChanged();
        run_then(
            self,
            async move { core.list_images(&folder).await },
            move |me, res| {
                if me.generation != gen {
                    return;
                }
                me.loading = false;
                match res {
                    Ok(items) => me.replace(items),
                    Err(e) => {
                        me.errorKind = qstr(e.kind.name());
                        me.errorMessage = qstr(&e.message);
                    }
                }
                me.stateChanged();
            },
        );
    }

    fn replace(&mut self, items: Vec<ImageItem>) {
        self.begin_reset_model();
        self.count = i32::try_from(items.len()).unwrap_or(i32::MAX);
        self.items = items;
        self.end_reset_model();
    }

    fn indexOf(&self, uri: QString) -> i32 {
        let Some(u) = parse_uri(&uri) else {
            return -1;
        };
        self.items
            .iter()
            .position(|i| i.uri == u)
            .and_then(|p| i32::try_from(p).ok())
            .unwrap_or(-1)
    }
}

impl QAbstractListModel for ImageListModel {
    fn row_count(&self) -> i32 {
        self.count
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(item) = usize::try_from(index.row()).ok().and_then(|r| self.items.get(r)) else {
            return QVariant::default();
        };
        match role {
            ROLE_URI => qstr(&item.uri.to_string()).into(),
            ROLE_NAME => qstr(&item.name).into(),
            ROLE_SIZE => item.size.map_or(-1.0, |s| s as f64).into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut m = HashMap::new();
        m.insert(ROLE_URI, "uri".into());
        m.insert(ROLE_NAME, "name".into());
        m.insert(ROLE_SIZE, "size".into());
        m
    }
}

// --- image providers ---------------------------------------------------

cpp! {{
    #include <QtQuick/QQuickImageProvider>
    #include <QtQuick/QQuickTextureFactory>
    #include <QtQml/QQmlEngine>
    #include <QtGui/QImage>
    #include <QtGui/QImageReader>
    #include <QtCore/QBuffer>
    #include <QtCore/QMutex>
    #include <QtCore/QMutexLocker>
    #include <QtCore/QMetaObject>
    #include <memory>

    // State shared by the response (GUI/reader thread) and the Rust job.
    struct LauttaImageState {
        QMutex mutex;
        QImage image;
        QString error;
        bool canceled = false;
        QObject *response = nullptr;
        quint64 id = 0;
    };

    static void lauttaImageCancelRequest(quint64 id);

    class LauttaImageResponse : public QQuickImageResponse {
    public:
        explicit LauttaImageResponse(std::shared_ptr<LauttaImageState> s) : state(std::move(s)) {}
        ~LauttaImageResponse() override { detach(); }

        QQuickTextureFactory *textureFactory() const override
        {
            return QQuickTextureFactory::textureFactoryForImage(state->image);
        }
        QString errorString() const override { return state->error; }
        void cancel() override
        {
            detach();
            lauttaImageCancelRequest(state->id);
        }

        std::shared_ptr<LauttaImageState> state;

    private:
        void detach()
        {
            QMutexLocker lock(&state->mutex);
            state->canceled = true;
            state->response = nullptr;
        }
    };

    // Called by the Rust job: decodes `data` (or records `err`) and finishes
    // the response unless it was cancelled. Takes over `handle`.
    static void lauttaImageDone(void *handle, const char *data, size_t len, int w, int h,
                                int orientation, bool transform, const char *err, size_t errLen)
    {
        std::unique_ptr<std::shared_ptr<LauttaImageState>> owner(
            static_cast<std::shared_ptr<LauttaImageState> *>(handle));
        LauttaImageState *s = owner->get();
        QImage image;
        QString error;
        if (err) {
            error = QString::fromUtf8(err, static_cast<int>(errLen));
        } else {
            QByteArray bytes = QByteArray::fromRawData(data, static_cast<int>(len));
            QBuffer buffer(&bytes);
            buffer.open(QIODevice::ReadOnly);
            QImageReader reader(&buffer);
            reader.setAutoTransform(transform);
            QSize size = reader.size();
            if (size.isValid() && (w > 0 || h > 0)) {
                // The request is for the displayed (rotated) size; decode
                // scaled down, never up, keeping the aspect ratio.
                QSize shown = (transform && orientation >= 5) ? size.transposed() : size;
                qreal rw = w > 0 ? qreal(w) / shown.width() : 1e9;
                qreal rh = h > 0 ? qreal(h) / shown.height() : 1e9;
                qreal ratio = qMin(rw, rh);
                if (ratio < 1.0) {
                    QSize target(qMax(1, qRound(size.width() * ratio)), qMax(1, qRound(size.height() * ratio)));
                    reader.setScaledSize(target);
                }
            }
            image = reader.read();
            if (image.isNull())
                error = reader.errorString().isEmpty() ? QStringLiteral("not an image") : reader.errorString();
        }
        QMutexLocker lock(&s->mutex);
        s->image = image;
        s->error = error;
        if (!s->canceled && s->response)
            QMetaObject::invokeMethod(s->response, "finished", Qt::QueuedConnection);
    }

    class LauttaImageProvider : public QQuickAsyncImageProvider {
    public:
        explicit LauttaImageProvider(int kind) : m_kind(kind) {}

        QQuickImageResponse *requestImageResponse(const QString &id, const QSize &requestedSize) override
        {
            static quint64 counter = 0;
            auto state = std::make_shared<LauttaImageState>();
            state->id = ++counter;
            auto *response = new LauttaImageResponse(state);
            state->response = response;
            auto *handle = new std::shared_ptr<LauttaImageState>(state);
            const QByteArray utf8 = id.toUtf8();
            const char *idData = utf8.constData();
            const size_t idLen = static_cast<size_t>(utf8.size());
            const int kind = m_kind;
            const int w = requestedSize.width();
            const int h = requestedSize.height();
            const quint64 reqId = state->id;
            rust!(Lautta_image_request [kind: i32 as "int", idData: *const u8 as "const char*",
                                        idLen: usize as "size_t", w: i32 as "int", h: i32 as "int",
                                        reqId: u64 as "quint64", handle: *mut c_void as "void*"] {
                request(kind, idData, idLen, w, h, reqId, handle);
            });
            return response;
        }

    private:
        int m_kind;
    };

    static void lauttaImageCancelRequest(quint64 id)
    {
        rust!(Lautta_image_cancel [id: u64 as "quint64"] { cancel(id); });
    }

    // The one call the shell makes per QQmlEngine (viewers area, PRV-2).
    static void lauttaRegisterImageProviders(QQmlEngine *engine)
    {
        engine->addImageProvider(QStringLiteral("lautta-thumb"), new LauttaImageProvider(0));
        engine->addImageProvider(QStringLiteral("lautta-file"), new LauttaImageProvider(1));
    }
}}

/// Adds `image://lautta-thumb/` and `image://lautta-file/` to a
/// `QQmlEngine` (the engine takes ownership). Call once per engine, e.g.
/// `viewers::images::register_image_providers(view->engine())`.
///
/// # Safety
/// `engine` must point to a live `QQmlEngine`.
pub unsafe fn register_image_providers(engine: *mut c_void) {
    cpp!(unsafe [engine as "QQmlEngine*"] { lauttaRegisterImageProviders(engine); })
}

const KIND_THUMB: i32 = 0;

/// What cancels a request: the thumbnail ticket or the fetch task.
type Canceller = Box<dyn FnOnce() + Send>;

static CANCELLERS: Mutex<Option<HashMap<u64, Canceller>>> = Mutex::new(None);

fn cancellers() -> std::sync::MutexGuard<'static, Option<HashMap<u64, Canceller>>> {
    CANCELLERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The C++ side's shared state of one request; finishing it hands it back.
struct Reply(*mut c_void);

// SAFETY: the pointer is an owning handle only used to call
// `lauttaImageDone` once, which locks internally.
unsafe impl Send for Reply {}

impl Reply {
    fn done(self, bytes: &[u8], w: i32, h: i32, orientation: u8, transform: bool) {
        let handle = self.take();
        let (data, len) = (bytes.as_ptr(), bytes.len());
        let orientation = i32::from(orientation);
        // SAFETY: `handle` came from `requestImageResponse` and is consumed here.
        unsafe {
            cpp!([handle as "void*", data as "const char*", len as "size_t", w as "int", h as "int",
                  orientation as "int", transform as "bool"] {
                lauttaImageDone(handle, data, len, w, h, orientation, transform, nullptr, 0);
            })
        }
    }

    fn fail(self, message: &str) {
        let handle = self.take();
        let (data, len) = (message.as_ptr(), message.len());
        // SAFETY: as in `done`.
        unsafe {
            cpp!([handle as "void*", data as "const char*", len as "size_t"] {
                lauttaImageDone(handle, nullptr, 0, 0, 0, 1, false, data, len);
            })
        }
    }

    fn take(self) -> *mut c_void {
        let p = self.0;
        std::mem::forget(self);
        p
    }
}

impl Drop for Reply {
    /// A job that ended without answering (aborted) still frees the handle.
    fn drop(&mut self) {
        Reply(self.0).fail("canceled");
    }
}

fn finish_error(reply: Reply, id: u64, e: &lautta_core::Error) {
    cancellers().get_or_insert_with(HashMap::new).remove(&id);
    reply.fail(&format!("{}: {}", e.kind.name(), e.message));
}

fn request(kind: i32, id_data: *const u8, id_len: usize, w: i32, h: i32, req: u64, handle: *mut c_void) {
    let reply = Reply(handle);
    // SAFETY: the C++ caller keeps the UTF-8 bytes alive during this call.
    let id = unsafe { std::slice::from_raw_parts(id_data, id_len) };
    let id = String::from_utf8_lossy(id).into_owned();
    let Some(core) = core() else {
        reply.fail("the app is not ready");
        return;
    };
    if kind == KIND_THUMB {
        thumb_request(core, &id, req, reply);
    } else {
        file_request(core, &id, (w, h), req, reply);
    }
}

fn thumb_request(core: std::sync::Arc<lautta_core::app::Core>, id: &str, req: u64, reply: Reply) {
    let Some((uri, size)) = parse_thumb_id(id) else {
        reply.fail("bad thumbnail id");
        return;
    };
    let rt = handle();
    let previews = match core.previews(&rt) {
        Ok(p) => p,
        Err(e) => return finish_error(reply, req, &e),
    };
    let ticket = previews.request(core, uri, size);
    let rx = ticket.rx;
    let cancel = ticket.cancel;
    cancellers()
        .get_or_insert_with(HashMap::new)
        .insert(req, Box::new(move || cancel.cancel()));
    rt.spawn(async move {
        let res = rx.await;
        cancellers().get_or_insert_with(HashMap::new).remove(&req);
        match res {
            Ok(Ok(bytes)) => reply.done(&bytes, 0, 0, 1, false),
            Ok(Err(e)) => reply.fail(&format!("{}: {}", e.kind.name(), e.message)),
            Err(_) => reply.fail("canceled"),
        }
    });
}

fn file_request(
    core: std::sync::Arc<lautta_core::app::Core>,
    id: &str,
    size: (i32, i32),
    req: u64,
    reply: Reply,
) {
    let Some(uri) = parse_image_uri(id) else {
        reply.fail("bad image id");
        return;
    };
    let task = handle().spawn(async move {
        let res = core.full_image(&uri).await;
        cancellers().get_or_insert_with(HashMap::new).remove(&req);
        match res {
            Ok(img) => {
                let _ = tokio::task::spawn_blocking(move || {
                    reply.done(&img.bytes, size.0, size.1, img.orientation, true)
                })
                .await;
            }
            Err(e) => reply.fail(&format!("{}: {}", e.kind.name(), e.message)),
        }
    });
    let abort = task.abort_handle();
    cancellers()
        .get_or_insert_with(HashMap::new)
        .insert(req, Box::new(move || abort.abort()));
}

fn cancel(id: u64) {
    let canceller = cancellers().get_or_insert_with(HashMap::new).remove(&id);
    if let Some(c) = canceller {
        c();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_core::locations::{Location, LocationKind};
    use lautta_core::provider::memory::MemoryProvider;
    use std::sync::Arc;

    cpp! {{
        #include <QtGui/QGuiApplication>
        #include <QtCore/QEventLoop>
        #include <QtCore/QTimer>

        // Requests `id` from the named provider of a fresh engine and waits
        // for the answer; returns the error text ("" when it worked).
        static QString lauttaProbe(const char *provider, const QString &id, int rw, int rh, int *w, int *h)
        {
            static int argc = 1;
            static char arg0[] = "test";
            static char *argv[] = {arg0, nullptr};
            static QGuiApplication *app = new QGuiApplication(argc, argv);
            Q_UNUSED(app);
            QQmlEngine engine;
            lauttaRegisterImageProviders(&engine);
            auto *p = static_cast<LauttaImageProvider *>(engine.imageProvider(QString::fromLatin1(provider)));
            QQuickImageResponse *r = p->requestImageResponse(id, QSize(rw, rh));
            QEventLoop loop;
            QObject::connect(r, &QQuickImageResponse::finished, &loop, &QEventLoop::quit);
            QTimer::singleShot(15000, &loop, &QEventLoop::quit);
            loop.exec();
            QString error = r->errorString();
            QQuickTextureFactory *f = r->textureFactory();
            QSize size = f ? f->textureSize() : QSize();
            delete f;
            delete r;
            *w = size.width();
            *h = size.height();
            return error.isEmpty() && size.isEmpty() ? QStringLiteral("timeout") : error;
        }

        static QByteArray lauttaTestJpeg(int w, int h)
        {
            QImage img(w, h, QImage::Format_RGB32);
            img.fill(Qt::red);
            QByteArray out;
            QBuffer buffer(&out);
            buffer.open(QIODevice::WriteOnly);
            img.save(&buffer, "JPEG");
            return out;
        }
    }}

    fn probe(provider: &str, id: &str, req: (i32, i32)) -> (String, i32, i32) {
        let provider = std::ffi::CString::new(provider).unwrap();
        let id = QString::from(id);
        let (mut w, mut h) = (0i32, 0i32);
        let (pw, ph) = (&mut w as *mut i32, &mut h as *mut i32);
        let p = provider.as_ptr();
        let (rw, rh) = req;
        let err = unsafe {
            cpp!([p as "const char*", id as "QString", rw as "int", rh as "int", pw as "int*", ph as "int*"]
                 -> QString as "QString" {
                return lauttaProbe(p, id, rw, rh, pw, ph);
            })
        };
        (err.to_string(), w, h)
    }

    fn test_jpeg(w: i32, h: i32) -> Vec<u8> {
        let bytes = cpp!(unsafe [w as "int", h as "int"] -> QByteArray as "QByteArray" {
            return lauttaTestJpeg(w, h);
        });
        bytes.to_slice().to_vec()
    }

    /// The JPEG with an EXIF block that has only an orientation.
    fn with_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let mut tiff = vec![b'I', b'I', 0x2a, 0, 8, 0, 0, 0, 1, 0];
        tiff.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0]);
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let mut out = vec![0xff, 0xd8, 0xff, 0xe1];
        out.extend_from_slice(&u16::try_from(2 + 6 + tiff.len()).unwrap().to_be_bytes());
        out.extend_from_slice(b"Exif\0\0");
        out.extend_from_slice(&tiff);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    /// One test: Qt allows a single application object per process.
    #[test]
    fn providers_serve_remote_images() {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        let home = tempfile::tempdir().unwrap();
        let paths = lautta_core::paths::AppPaths::new(home.path());
        let core = crate::runtime::init(paths).unwrap();
        let nas = MemoryProvider::default();
        nas.add_file("p/a.jpg", &test_jpeg(200, 100), 1);
        nas.add_file("p/rot.jpg", &with_orientation(&test_jpeg(40, 20), 6), 1);
        nas.add_file("p/big.jpg", &test_jpeg(400, 200), 1);
        nas.add_file("p/not.jpg", b"this is not an image", 1);
        let loc = Location::remote(
            "nv-qt-images",
            LocationKind::Server {
                provider: "sftp".into(),
            },
            "NAS",
            None,
        );
        core.locations.register(loc, Arc::new(nas));

        let (e, w, h) = probe("lautta-thumb", "lautta://nv-qt-images/p/a.jpg?size=64", (0, 0));
        assert_eq!(
            (e.as_str(), w, h),
            ("", 64, 32),
            "thumbnail at the requested edge (PRV-2)"
        );

        let (e, w, h) = probe("lautta-file", "lautta://nv-qt-images/p/a.jpg", (0, 0));
        assert_eq!((e.as_str(), w, h), ("", 200, 100), "full image");

        let (e, w, h) = probe("lautta-file", "lautta://nv-qt-images/p/rot.jpg", (0, 0));
        assert_eq!((e.as_str(), w, h), ("", 20, 40), "EXIF orientation applied");

        let (e, w, h) = probe("lautta-file", "lautta://nv-qt-images/p/big.jpg", (100, 100));
        assert_eq!(
            (e.as_str(), w, h),
            ("", 100, 50),
            "scaled down to the request, aspect kept"
        );

        let (e, w, h) = probe("lautta-file", "lautta://nv-qt-images/p/big.jpg", (4000, 4000));
        assert_eq!((e.as_str(), w, h), ("", 400, 200), "never scaled up");

        let (e, ..) = probe("lautta-file", "lautta://nv-qt-images/p/missing.jpg", (0, 0));
        assert!(e.starts_with("NotFound"), "{e}");
        let (e, ..) = probe("lautta-file", "lautta://nv-qt-images/p/not.jpg", (0, 0));
        assert!(!e.is_empty() && !e.starts_with("timeout"), "{e}");
        let (e, ..) = probe("lautta-thumb", "lautta://nv-qt-images/p/a.jpg", (0, 0));
        assert!(e.contains("bad thumbnail id"), "{e}");
        let (e, ..) = probe("lautta-file", "garbage", (0, 0));
        assert!(e.contains("bad image id"), "{e}");
    }
}
