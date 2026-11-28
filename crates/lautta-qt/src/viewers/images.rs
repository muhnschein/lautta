// SPDX-License-Identifier: LGPL-2.1-or-later

use super::{parse_uri, qstr, run_then};
use crate::runtime::{core, handle};
use cpp::cpp;
use lautta_core::app_viewers::{
    parse_image_uri, parse_thumb_id, viewer_decode_size, GalleryItem, PreviewKind,
};
use lautta_core::error::ErrorKind;
use lautta_core::preview::svg::svg_refusal;
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::Mutex;

pub fn register() {
    qml_register_type::<GalleryListModel>(&crate::qml_uri(), 1, 0, &crate::cstr("GalleryListModel"));
}

const ROLE_URI: i32 = 0x100;
const ROLE_NAME: i32 = 0x101;
const ROLE_SIZE: i32 = 0x102;
const ROLE_VIDEO: i32 = 0x103;
const ROLE_MIME: i32 = 0x104;

#[derive(QObject, Default)]
pub struct GalleryListModel {
    base: qt_base_class!(trait QAbstractListModel),
    folderUri: qt_property!(QString; WRITE setFolderUri NOTIFY folderUriChanged),
    folderUriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    count: qt_property!(i32; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),
    indexOf: qt_method!(fn(&self, uri: QString) -> i32),
    reload: qt_method!(fn(&mut self)),
    items: Vec<GalleryItem>,
    generation: u64,
}

impl Drop for GalleryListModel {
    fn drop(&mut self) {
        if let Some(core) = core() {
            core.forget_shown_images();
        }
    }
}

impl GalleryListModel {
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
            async move { core.list_gallery(&folder).await },
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

    fn replace(&mut self, items: Vec<GalleryItem>) {
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

impl QAbstractListModel for GalleryListModel {
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
            ROLE_VIDEO => item.video.into(),
            ROLE_MIME => qstr(&item.mime).into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut m = HashMap::new();
        m.insert(ROLE_URI, "uri".into());
        m.insert(ROLE_NAME, "name".into());
        m.insert(ROLE_SIZE, "size".into());
        m.insert(ROLE_VIDEO, "video".into());
        m.insert(ROLE_MIME, "mimeType".into());
        m
    }
}

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

    static void lauttaImageDone(void *handle, const char *data, size_t len, int w, int h,
                                int orientation, bool transform, bool exact, const char *err,
                                size_t errLen)
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
            if (exact) {
                if (w > 0 && h > 0)
                    reader.setScaledSize(QSize(w, h));
            } else if (size.isValid() && (w > 0 || h > 0)) {
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

    static void lauttaImageDoneRgba(void *handle, const uchar *data, int w, int h)
    {
        std::unique_ptr<std::shared_ptr<LauttaImageState>> owner(
            static_cast<std::shared_ptr<LauttaImageState> *>(handle));
        LauttaImageState *s = owner->get();
        QImage image = QImage(data, w, h, w * 4, QImage::Format_RGBA8888_Premultiplied).copy();
        QMutexLocker lock(&s->mutex);
        s->image = image;
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

    static void lauttaRegisterImageProviders(QQmlEngine *engine)
    {
        engine->addImageProvider(QStringLiteral("lautta-thumb"), new LauttaImageProvider(0));
        engine->addImageProvider(QStringLiteral("lautta-file"), new LauttaImageProvider(1));
        engine->addImageProvider(QStringLiteral("lautta-pdf"), new LauttaImageProvider(2));
        engine->addImageProvider(QStringLiteral("lautta-cover"), new LauttaImageProvider(3));
        engine->addImageProvider(QStringLiteral("lautta-pdf-thumb"), new LauttaImageProvider(4));
        engine->addImageProvider(QStringLiteral("lautta-icon"), new LauttaIconProvider());
    }
}}

/// # Safety
/// `engine` must point to a live `QQmlEngine`.
pub unsafe fn register_image_providers(engine: *mut c_void) {
    cpp!(unsafe [engine as "QQmlEngine*"] { lauttaRegisterImageProviders(engine); })
}

const KIND_THUMB: i32 = 0;
const KIND_PDF: i32 = 2;
const KIND_COVER: i32 = 3;
const KIND_PDF_THUMB: i32 = 4;

pub(crate) type Canceller = Box<dyn FnOnce() + Send>;

static CANCELLERS: Mutex<Option<HashMap<u64, Canceller>>> = Mutex::new(None);

fn cancellers() -> std::sync::MutexGuard<'static, Option<HashMap<u64, Canceller>>> {
    CANCELLERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) struct Reply(*mut c_void);

unsafe impl Send for Reply {}

impl Reply {
    fn done(self, bytes: &[u8], (w, h): (i32, i32), orientation: u8, transform: bool, exact: bool) {
        if let Some(why) = svg_refusal(bytes) {
            return self.fail(&format!("{}: {}", ErrorKind::Unsupported.name(), why.message()));
        }
        let handle = self.take();
        let (data, len) = (bytes.as_ptr(), bytes.len());
        let orientation = i32::from(orientation);
        unsafe {
            cpp!([handle as "void*", data as "const char*", len as "size_t", w as "int", h as "int",
                  orientation as "int", transform as "bool", exact as "bool"] {
                lauttaImageDone(handle, data, len, w, h, orientation, transform, exact, nullptr, 0);
            })
        }
    }

    pub(crate) fn done_rgba(self, rgba: &[u8], w: u32, h: u32) {
        let (Ok(w), Ok(h)) = (i32::try_from(w), i32::try_from(h)) else {
            return self.fail("page too large");
        };
        if rgba.len() != (w as usize) * (h as usize) * 4 {
            return self.fail("bad page buffer");
        }
        let handle = self.take();
        let data = rgba.as_ptr();
        unsafe {
            cpp!([handle as "void*", data as "const uchar*", w as "int", h as "int"] {
                lauttaImageDoneRgba(handle, data, w, h);
            })
        }
    }

    pub(crate) fn fail(self, message: &str) {
        let handle = self.take();
        let (data, len) = (message.as_ptr(), message.len());
        unsafe {
            cpp!([handle as "void*", data as "const char*", len as "size_t"] {
                lauttaImageDone(handle, nullptr, 0, 0, 0, 1, false, false, data, len);
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
    fn drop(&mut self) {
        Reply(self.0).fail("canceled");
    }
}

pub(crate) fn on_cancel(id: u64, canceller: Canceller) {
    cancellers()
        .get_or_insert_with(HashMap::new)
        .insert(id, canceller);
}

pub(crate) fn finished(id: u64) {
    cancellers().get_or_insert_with(HashMap::new).remove(&id);
}

fn finish_error(reply: Reply, id: u64, e: &lautta_core::Error) {
    cancellers().get_or_insert_with(HashMap::new).remove(&id);
    reply.fail(&format!("{}: {}", e.kind.name(), e.message));
}

fn request(kind: i32, id_data: *const u8, id_len: usize, w: i32, h: i32, req: u64, handle: *mut c_void) {
    let reply = Reply(handle);
    let id = unsafe { std::slice::from_raw_parts(id_data, id_len) };
    let id = String::from_utf8_lossy(id).into_owned();
    let Some(core) = core() else {
        reply.fail("the app is not ready");
        return;
    };
    if kind == KIND_THUMB || kind == KIND_COVER {
        thumb_request(core, &id, kind, (w, h), req, reply);
    } else if kind == KIND_PDF {
        super::pdf::page_request(core, &id, w, req, reply);
    } else if kind == KIND_PDF_THUMB {
        super::pdf::thumb_request(core, &id, w, req, reply);
    } else {
        file_request(core, &id, (w, h), req, reply);
    }
}

fn thumb_request(
    core: std::sync::Arc<lautta_core::app::Core>,
    id: &str,
    kind: i32,
    shown: (i32, i32),
    req: u64,
    reply: Reply,
) {
    let (id, preview) = preview_kind(id, kind);
    let Some(thumb) = parse_thumb_id(id) else {
        reply.fail("bad thumbnail id");
        return;
    };
    let rt = handle();
    if thumb.embedded {
        let task = rt.spawn(async move {
            let res = core.viewer_preview(&thumb.uri, thumb.size).await;
            cancellers().get_or_insert_with(HashMap::new).remove(&req);
            match res {
                Ok(bytes) => reply.done(&bytes, shown, 1, false, false),
                Err(e) => reply.fail(&format!("{}: {}", e.kind.name(), e.message)),
            }
        });
        let abort = task.abort_handle();
        on_cancel(req, Box::new(move || abort.abort()));
        return;
    }
    let previews = match core.previews(&rt) {
        Ok(p) => p,
        Err(e) => return finish_error(reply, req, &e),
    };
    let ticket = match preview {
        PreviewKind::Thumb if thumb.cached => previews.request_cached(core, thumb.uri, thumb.size),
        PreviewKind::Thumb => previews.request(core, thumb.uri, thumb.size),
        cover => previews.request_cover(core, thumb.uri, thumb.size, cover),
    };
    let rx = ticket.rx;
    let cancel = ticket.cancel;
    cancellers()
        .get_or_insert_with(HashMap::new)
        .insert(req, Box::new(move || cancel.cancel()));
    rt.spawn(async move {
        let res = rx.await;
        cancellers().get_or_insert_with(HashMap::new).remove(&req);
        match res {
            Ok(Ok(bytes)) => reply.done(&bytes, shown, 1, false, false),
            Ok(Err(e)) => reply.fail(&format!("{}: {}", e.kind.name(), e.message)),
            Err(_) => reply.fail("canceled"),
        }
    });
}

fn preview_kind(id: &str, kind: i32) -> (&str, PreviewKind) {
    if kind != KIND_COVER {
        return (id, PreviewKind::Thumb);
    }
    match id.strip_suffix("&glow=1") {
        Some(rest) => (rest, PreviewKind::Glow),
        None => (id, PreviewKind::Cover),
    }
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
                let _ = tokio::task::spawn_blocking(move || match screen_decode_size(&img.bytes, size) {
                    Some(decode) => reply.done(&img.bytes, decode, img.orientation, true, true),
                    None => reply.done(&img.bytes, size, img.orientation, true, false),
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

fn screen_decode_size(bytes: &[u8], (w, h): (i32, i32)) -> Option<(i32, i32)> {
    let screen = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
    if screen.0 == 0 || screen.1 == 0 {
        return None;
    }
    let side = |v: u32| i32::try_from(v).unwrap_or(0);
    Some(viewer_decode_size(bytes, screen).map_or((0, 0), |(dw, dh)| (side(dw), side(dh))))
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

        static QByteArray lauttaTestImage(int w, int h, const char *format)
        {
            QImage img(w, h, QImage::Format_RGB32);
            img.fill(Qt::red);
            QByteArray out;
            QBuffer buffer(&out);
            buffer.open(QIODevice::WriteOnly);
            img.save(&buffer, format);
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

    fn test_image(w: i32, h: i32, format: &str) -> Vec<u8> {
        let format = std::ffi::CString::new(format).unwrap();
        let f = format.as_ptr();
        let bytes = cpp!(unsafe [w as "int", h as "int", f as "const char*"] -> QByteArray as "QByteArray" {
            return lauttaTestImage(w, h, f);
        });
        bytes.to_slice().to_vec()
    }

    fn nested_svg(depth: usize) -> Vec<u8> {
        let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200">"#);
        svg.push_str(&"<g>".repeat(depth - 1));
        svg.push_str(r#"<rect width="400" height="200" fill="red"/>"#);
        svg.push_str(&"</g>".repeat(depth - 1));
        svg.push_str("</svg>");
        svg.into_bytes()
    }

    fn test_jpeg(w: i32, h: i32) -> Vec<u8> {
        test_image(w, h, "JPEG")
    }

    fn with_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        with_exif(jpeg, orientation, None)
    }

    fn with_exif(jpeg: &[u8], orientation: u16, thumb: Option<&[u8]>) -> Vec<u8> {
        let mut tiff = vec![b'I', b'I', 0x2a, 0, 8, 0, 0, 0, 1, 0];
        tiff.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0]);
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]);
        match thumb {
            None => tiff.extend_from_slice(&[0, 0, 0, 0]),
            Some(t) => {
                tiff.extend_from_slice(&26u32.to_le_bytes());
                tiff.extend_from_slice(&[2, 0, 0x01, 0x02, 4, 0, 1, 0, 0, 0]);
                tiff.extend_from_slice(&56u32.to_le_bytes());
                tiff.extend_from_slice(&[0x02, 0x02, 4, 0, 1, 0, 0, 0]);
                tiff.extend_from_slice(&u32::try_from(t.len()).unwrap().to_le_bytes());
                tiff.extend_from_slice(&[0, 0, 0, 0]);
                tiff.extend_from_slice(t);
            }
        }
        let mut out = vec![0xff, 0xd8, 0xff, 0xe1];
        out.extend_from_slice(&u16::try_from(2 + 6 + tiff.len()).unwrap().to_be_bytes());
        out.extend_from_slice(b"Exif\0\0");
        out.extend_from_slice(&tiff);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

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
        nas.add_file("p/photo.jpg", &test_jpeg(800, 600), 1);
        nas.add_file("p/photo-rot.jpg", &with_orientation(&test_jpeg(800, 600), 6), 1);
        nas.add_file("p/photo.png", &test_image(800, 600, "PNG"), 1);
        let thumb = test_jpeg(40, 30);
        nas.add_file("p/cam.jpg", &with_exif(&test_jpeg(800, 600), 6, Some(&thumb)), 1);
        let loc = Location::remote(
            "nv-qt-images",
            LocationKind::Server {
                provider: "sftp".into(),
            },
            "NAS",
            None,
        );
        core.locations.register(loc, Arc::new(nas.clone()));

        let (e, w, h) = probe(
            "lautta-thumb",
            "lautta://nv-qt-images/p/a.jpg?size=64&cached=1",
            (0, 0),
        );
        assert_eq!((w, h), (-1, -1), "none cached yet: {e}");
        assert!(e.starts_with("NotFound"), "{e}");

        let (e, w, h) = probe("lautta-thumb", "lautta://nv-qt-images/p/a.jpg?size=64", (0, 0));
        assert_eq!(
            (e.as_str(), w, h),
            ("", 128, 64),
            "thumbnail at the size covering the request"
        );
        let (e, w, h) = probe("lautta-thumb", "lautta://nv-qt-images/p/a.jpg?size=64", (64, 64));
        assert_eq!((e.as_str(), w, h), ("", 64, 32), "decoded at the item's size");
        let (e, w, h) = probe(
            "lautta-thumb",
            "lautta://nv-qt-images/p/a.jpg?size=1024&cached=1",
            (0, 0),
        );
        assert_eq!((e.as_str(), w, h), ("", 128, 64), "largest cached one");

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
        let (e, w, h) = probe("lautta-file", "lautta://nv-qt-images/p/big.jpg", (100, 0));
        assert_eq!((e.as_str(), w, h), ("", 100, 50), "width alone: fit to it");

        let (e, w, h) = probe("lautta-file", "lautta://nv-qt-images/p/photo.jpg", (108, 228));
        assert_eq!(
            (e.as_str(), w, h),
            ("", 200, 150),
            "JPEG at the quarter step covering it"
        );
        let (e, w, h) = probe("lautta-file", "lautta://nv-qt-images/p/photo-rot.jpg", (108, 228));
        assert_eq!((e.as_str(), w, h), ("", 150, 200), "step, then turned upright");
        let (e, w, h) = probe("lautta-file", "lautta://nv-qt-images/p/photo.png", (108, 228));
        assert_eq!((e.as_str(), w, h), ("", 144, 108), "other formats at the fit");

        let (e, w, h) = probe(
            "lautta-thumb",
            "lautta://nv-qt-images/p/cam.jpg?size=1024&embedded=1",
            (0, 0),
        );
        assert_eq!(
            (e.as_str(), w, h),
            ("", 30, 40),
            "embedded thumbnail, turned upright"
        );
        let (e, w, h) = probe(
            "lautta-thumb",
            "lautta://nv-qt-images/p/photo-rot.jpg?size=1024&embedded=1",
            (0, 0),
        );
        assert_eq!(
            (e.as_str(), w, h),
            ("", 192, 256),
            "no camera thumbnail: the upright shape"
        );

        let (e, ..) = probe("lautta-file", "lautta://nv-qt-images/p/missing.jpg", (0, 0));
        assert!(e.starts_with("NotFound"), "{e}");
        let (e, ..) = probe("lautta-file", "lautta://nv-qt-images/p/not.jpg", (0, 0));
        assert!(!e.is_empty() && !e.starts_with("timeout"), "{e}");
        let (e, ..) = probe("lautta-thumb", "lautta://nv-qt-images/p/a.jpg", (0, 0));
        assert!(e.contains("bad thumbnail id"), "{e}");
        let (e, ..) = probe("lautta-file", "garbage", (0, 0));
        assert!(e.contains("bad image id"), "{e}");

        let at_limit = nested_svg(lautta_core::preview::svg::MAX_SVG_DEPTH);
        nas.add_file("p/at-limit.svg", &at_limit, 1);
        nas.add_file("p/deep.svg", &nested_svg(100_000), 1);
        let (e, w, h) = probe("lautta-file", "lautta://nv-qt-images/p/at-limit.svg", (0, 0));
        assert_eq!(
            (e.as_str(), w, h),
            ("", 400, 200),
            "an SVG nested up to the limit is drawn"
        );
        let (e, w, h) = probe(
            "lautta-thumb",
            "lautta://nv-qt-images/p/at-limit.svg?size=64",
            (64, 64),
        );
        assert_eq!(
            (e.as_str(), w, h),
            ("", 64, 32),
            "its thumbnail is drawn at the item's size"
        );
        let refused = "Unsupported: image nested too deeply to show";
        let (e, ..) = probe("lautta-file", "lautta://nv-qt-images/p/deep.svg", (0, 0));
        assert_eq!(e, refused, "100000 nested groups are refused, not given to Qt");
        let (e, ..) = probe(
            "lautta-thumb",
            "lautta://nv-qt-images/p/deep.svg?size=64",
            (64, 64),
        );
        assert_eq!(e, refused, "and get no thumbnail");
        let mut huge = vec![b' '; usize::try_from(lautta_core::thumbs::MAX_SVG_THUMB_BYTES).unwrap()];
        huge.extend(nested_svg(1));
        nas.add_file("p/huge.svg", &huge, 1);
        let (e, ..) = probe(
            "lautta-thumb",
            "lautta://nv-qt-images/p/huge.svg?size=64",
            (64, 64),
        );
        assert!(e.ends_with("too large for a thumbnail"), "{e}");

        nas.add_file(
            "p/two.pdf",
            &lautta_pdf::sample::build(&[
                lautta_pdf::sample::page(100, 200, lautta_pdf::sample::RED_SQUARE),
                lautta_pdf::sample::page(300, 150, ""),
            ]),
            1,
        );
        nas.add_file("p/fake.pdf", b"not a pdf", 1);
        let (e, w, h) = probe("lautta-pdf", "lautta://nv-qt-images/p/two.pdf?page=0", (50, 0));
        assert_eq!((e.as_str(), w, h), ("", 50, 100), "page at the asked width");
        let (e, w, h) = probe("lautta-pdf", "lautta://nv-qt-images/p/two.pdf?page=1", (0, 0));
        assert_eq!(
            (e.as_str(), w, h),
            ("", 1080, 540),
            "the default width without a request"
        );
        let (e, ..) = probe("lautta-pdf", "lautta://nv-qt-images/p/two.pdf?page=2", (50, 0));
        assert_eq!(e, "no such page");
        let (e, ..) = probe("lautta-pdf", "lautta://nv-qt-images/p/fake.pdf?page=0", (50, 0));
        assert_eq!(e, "not a readable PDF");
        let (e, ..) = probe("lautta-pdf", "lautta://nv-qt-images/p/gone.pdf?page=0", (50, 0));
        assert!(e.starts_with("NotFound"), "{e}");
        let (e, ..) = probe("lautta-pdf", "lautta://nv-qt-images/p/two.pdf", (50, 0));
        assert_eq!(e, "bad page id");
        let (e, w, h) = probe(
            "lautta-pdf-thumb",
            "lautta://nv-qt-images/p/two.pdf?page=0",
            (120, 0),
        );
        assert_eq!(
            (e.as_str(), w, h),
            ("", 90, 180),
            "page thumbnail fits a tile 1.5 times as tall as wide"
        );
        let (e, w, h) = probe(
            "lautta-pdf-thumb",
            "lautta://nv-qt-images/p/two.pdf?page=1",
            (0, 0),
        );
        assert_eq!(
            (e.as_str(), w, h),
            ("", 400, 200),
            "the thumbnail cap without a request"
        );
        let (e, ..) = probe(
            "lautta-pdf-thumb",
            "lautta://nv-qt-images/p/two.pdf?page=2",
            (50, 0),
        );
        assert_eq!(e, "no such page");
        let (e, ..) = probe("lautta-pdf-thumb", "lautta://nv-qt-images/p/two.pdf", (50, 0));
        assert_eq!(e, "bad page id");

        let (e, w, h) = probe(
            "lautta-thumb",
            "lautta://nv-qt-images/p/two.pdf?size=64",
            (64, 64),
        );
        assert_eq!((e.as_str(), w, h), ("", 64, 64), "first page as wide as asked");
        let (e, ..) = probe("lautta-thumb", "lautta://nv-qt-images/p/fake.pdf?size=64", (0, 0));
        assert!(e.contains("not a readable PDF"), "{e}");

        nas.add_file("m/song.mp3", &mp3_with_cover(&test_jpeg(300, 300)), 1);
        let (e, w, h) = probe(
            "lautta-cover",
            "lautta://nv-qt-images/m/song.mp3?size=200",
            (0, 0),
        );
        assert_eq!(
            (e.as_str(), w, h),
            ("", 256, 256),
            "cover at the size covering the request"
        );
        let (e, w, h) = probe(
            "lautta-cover",
            "lautta://nv-qt-images/m/song.mp3?size=200&glow=1",
            (0, 0),
        );
        assert_eq!((e.as_str(), w, h), ("", 48, 48), "glow");
        let (e, w, h) = probe("lautta-thumb", "lautta://nv-qt-images/m/song.mp3?size=64", (0, 0));
        assert_eq!((e.as_str(), w, h), ("", 128, 128), "audio thumbnail");
        let (e, ..) = probe("lautta-cover", "lautta://nv-qt-images/p/a.jpg?size=64", (0, 0));
        assert!(e.starts_with("Unsupported"), "covers only for audio: {e}");
        let (e, ..) = probe("lautta-cover", "lautta://nv-qt-images/m/song.mp3&glow=1", (0, 0));
        assert!(e.contains("bad thumbnail id"), "{e}");
    }

    fn mp3_with_cover(jpeg: &[u8]) -> Vec<u8> {
        let safe = |n: usize| {
            [
                (n >> 21) as u8 & 0x7f,
                (n >> 14) as u8 & 0x7f,
                (n >> 7) as u8 & 0x7f,
                n as u8 & 0x7f,
            ]
        };
        let mut body = b"\x03image/jpeg\0\x03\0".to_vec();
        body.extend_from_slice(jpeg);
        let mut frame = b"APIC".to_vec();
        frame.extend_from_slice(&safe(body.len()));
        frame.extend_from_slice(&[0, 0]);
        frame.extend_from_slice(&body);
        let mut out = b"ID3\x04\0\0".to_vec();
        out.extend_from_slice(&safe(frame.len()));
        out.extend_from_slice(&frame);
        for _ in 0..8 {
            out.extend_from_slice(&[0xff, 0xfb, 0x90, 0x64]);
            out.extend(std::iter::repeat_n(0u8, 413));
        }
        out
    }
}
