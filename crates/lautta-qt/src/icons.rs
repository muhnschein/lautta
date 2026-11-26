// SPDX-License-Identifier: LGPL-2.1-or-later

use cpp::cpp;
use qmetaobject::QByteArray;

cpp! {{
    #include <QtQuick/QQuickImageProvider>
    #include <QtCore/QBuffer>
    #include <QtCore/QFile>
    #include <QtCore/QUrlQuery>
    #include <QtGui/QImage>
    #include <QtGui/QImageReader>

    static QImage lauttaRenderIcon(const QString &path, int side, double stroke)
    {
        QFile file(path);
        if (!file.open(QIODevice::ReadOnly))
            return QImage();
        QByteArray svg = file.readAll();
        const char *data = svg.constData();
        const size_t len = static_cast<size_t>(svg.size());
        const double px = side;
        QByteArray restroked = rust!(Lautta_icon_restroke [data: *const u8 as "const char*",
                                                           len: usize as "size_t", px: f64 as "double",
                                                           stroke: f64 as "double"] -> QByteArray as "QByteArray" {
            restroked(data, len, px, stroke)
        });
        if (!restroked.isEmpty())
            svg = restroked;
        QBuffer buffer(&svg);
        buffer.open(QIODevice::ReadOnly);
        QImageReader reader(&buffer, "svg");
        if (side > 0)
            reader.setScaledSize(QSize(side, side));
        return reader.read();
    }

    class LauttaIconProvider : public QQuickImageProvider {
    public:
        LauttaIconProvider() : QQuickImageProvider(QQuickImageProvider::Image) {}

        QImage requestImage(const QString &id, QSize *size, const QSize &requestedSize) override
        {
            const int q = id.indexOf(QLatin1Char('?'));
            const QString path = QLatin1Char('/') + id.left(q);
            const double stroke = q < 0 ? 0.0 : QUrlQuery(id.mid(q + 1)).queryItemValue(QStringLiteral("stroke")).toDouble();
            QImage image = lauttaRenderIcon(path, requestedSize.width(), stroke);
            if (size)
                *size = image.size();
            return image;
        }
    };
}}

fn restroked(data: *const u8, len: usize, side_px: f64, stroke_px: f64) -> QByteArray {
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|svg| restroke(svg, side_px, stroke_px))
        .map_or_else(QByteArray::default, |s| QByteArray::from(s.as_str()))
}

pub fn restroke(svg: &str, side_px: f64, stroke_px: f64) -> Option<String> {
    if !(side_px > 0.0 && stroke_px > 0.0) {
        return None;
    }
    let root_start = svg.find("<svg")?;
    let root = &svg[root_start..root_start + svg[root_start..].find('>')?];
    let width: f64 = attr(root, "viewBox")?.split_whitespace().nth(2)?.parse().ok()?;
    let outline: f64 = attr(root, "stroke-width")?.parse().ok()?;
    if !(width > 0.0 && outline > 0.0) {
        return None;
    }
    let factor = stroke_px * width / side_px / outline;
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    const KEY: &str = " stroke-width=\"";
    while let Some(at) = rest.find(KEY) {
        let (head, tail) = rest.split_at(at + KEY.len());
        let end = tail.find('"')?;
        let value: f64 = tail[..end].parse().ok()?;
        out.push_str(head);
        out.push_str(&format!("{}", (value * factor * 10_000.0).round() / 10_000.0));
        rest = &tail[end..];
    }
    out.push_str(rest);
    Some(out)
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    let len = tag[start..].find('"')?;
    Some(&tag[start..start + len])
}

#[cfg(test)]
mod tests {
    use super::restroke;
    use cpp::cpp;

    fn drawn(file: &str, side: i32, stroke: f64) -> Vec<u8> {
        let dir = std::fs::canonicalize(format!("{}/../../qml/icons", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let id = format!("{}/{file}?stroke={stroke}", dir.display());
        let id = qmetaobject::QString::from(id.trim_start_matches('/'));
        let mut out = vec![0u8; (side * side) as usize];
        let buf = out.as_mut_ptr();
        let ok = cpp!(unsafe [id as "QString", side as "int", buf as "uchar*"] -> bool as "bool" {
            QSize size;
            QImage image = LauttaIconProvider().requestImage(id, &size, QSize(side, side)).convertToFormat(QImage::Format_ARGB32);
            if (image.size() != QSize(side, side) || size != image.size())
                return false;
            for (int y = 0; y < side; ++y)
                for (int x = 0; x < side; ++x)
                    buf[y * side + x] = qAlpha(image.pixel(x, y));
            return true;
        });
        if ok {
            out
        } else {
            Vec::new()
        }
    }

    fn line_width(alpha: &[u8], side: usize) -> f64 {
        let mut widths = std::collections::HashMap::<i64, usize>::new();
        for horizontal in [true, false] {
            for i in 0..side {
                let (mut sum, mut peak) = (0.0, 0.0_f64);
                for j in 0..=side {
                    let a = match (j == side, horizontal) {
                        (true, _) => 0,
                        (false, true) => alpha[i * side + j],
                        (false, false) => alpha[j * side + i],
                    };
                    if a > 5 {
                        sum += f64::from(a);
                        peak = peak.max(f64::from(a));
                    } else if sum > 0.0 {
                        let w = sum / peak;
                        if w < side as f64 / 10.0 {
                            *widths.entry((w * 4.0).round() as i64).or_default() += 1;
                        }
                        (sum, peak) = (0.0, 0.0);
                    }
                }
            }
        }
        widths
            .into_iter()
            .max_by_key(|&(_, n)| n)
            .map_or(0.0, |(w, _)| w as f64 / 4.0)
    }

    const ICON: &str = r##"<!-- c --><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="none" stroke="#ffffff" stroke-width="1"><path d="M1 1h2"/><path d="M4 4h2" stroke-width="2"/></svg>"##;

    #[test]
    fn outline_comes_out_stroke_px_wide_at_any_size() {
        assert_eq!(restroke(ICON, 96.0, 3.0).as_deref(), Some(ICON));
        let out = restroke(ICON, 168.0, 3.0).unwrap();
        assert!(
            out.contains(r##"stroke="#ffffff" stroke-width="0.5714">"##),
            "{out}"
        );
        assert!(
            out.contains(r#"<path d="M4 4h2" stroke-width="1.1429"/>"#),
            "{out}"
        );
        let out = restroke(ICON, 112.0, 3.5).unwrap();
        assert!(
            out.contains(r#"stroke-width="1">"#) && out.contains(r#"stroke-width="2"/>"#),
            "{out}"
        );
    }

    #[test]
    fn scales_against_the_root_outline_and_the_viewbox_width() {
        let wide = ICON.replace(r#"viewBox="0 0 32 32""#, r#"viewBox="0 0 64 64""#);
        let out = restroke(&wide, 96.0, 3.0).unwrap();
        assert!(
            out.contains(r#"stroke-width="2">"#) && out.contains(r#"stroke-width="4"/>"#),
            "{out}"
        );
        let heavy = ICON.replace(
            r##"stroke="#ffffff" stroke-width="1""##,
            r##"stroke="#ffffff" stroke-width="2""##,
        );
        let out = restroke(&heavy, 96.0, 3.0).unwrap();
        assert!(
            out.contains(r#"stroke-width="1">"#) && out.contains(r#"stroke-width="1"/>"#),
            "{out}"
        );
    }

    #[test]
    fn leaves_the_rest_of_the_file_alone() {
        let want = ICON
            .replace(r#"stroke-width="1""#, r#"stroke-width="0.5""#)
            .replace(r#"stroke-width="2""#, r#"stroke-width="1""#);
        assert_eq!(restroke(ICON, 192.0, 3.0).as_deref(), Some(want.as_str()));
    }

    #[test]
    fn provider_draws_every_outline_at_the_asked_width() {
        for (name, side, stroke) in [
            ("server.svg", 96, 3.0),
            ("checksum.svg", 96, 3.0),
            ("server.svg", 48, 3.0),
            ("lock.svg", 144, 3.0),
            ("file-folder-28.svg", 168, 3.0),
            ("file-document-28.svg", 168, 3.0),
            ("file-document-32.svg", 96, 3.0),
            ("file-spreadsheet-28.svg", 168, 3.0),
            ("file-presentation-32.svg", 96, 3.0),
            ("file-key-32.svg", 96, 3.0),
            ("file-disk-28.svg", 168, 3.0),
            ("view-options.svg", 96, 3.0),
            ("file-folder-28.svg", 112, 3.5),
            ("dir-copy.svg", 128, 4.0),
            ("home.svg", 72, 3.0),
            ("home.svg", 48, 2.0),
        ] {
            let alpha = drawn(name, side, stroke);
            assert!(!alpha.is_empty(), "{name} at {side} px not drawn");
            let w = line_width(&alpha, side as usize);
            assert!(
                (w - stroke).abs() <= 0.5,
                "{name} at {side} px: lines {w} px, want {stroke}"
            );
        }
    }

    #[test]
    fn provider_draws_nothing_but_svg_files() {
        assert!(drawn("lautta-256.png", 96, 3.0).is_empty());
    }

    #[test]
    fn fill_only_icon_or_bad_input_is_drawn_as_is() {
        let fill_only = r#"<svg viewBox="0 0 16 16"><rect stroke-width="1"/></svg>"#;
        assert_eq!(restroke(fill_only, 48.0, 3.0), None, "no root stroke-width");
        let no_box = ICON.replace(r#" viewBox="0 0 32 32""#, "");
        assert_eq!(restroke(&no_box, 96.0, 3.0), None, "no viewBox");
        assert_eq!(restroke(ICON, 0.0, 3.0), None, "no size");
        assert_eq!(restroke(ICON, 96.0, 0.0), None, "no stroke");
        assert_eq!(restroke(ICON, -96.0, 3.0), None, "negative size");
        assert_eq!(
            restroke(&ICON.replace("32 32", "0 32"), 96.0, 3.0),
            None,
            "zero viewBox width"
        );
        assert_eq!(
            restroke(
                &ICON.replace(r#"stroke-width="1""#, r#"stroke-width="0""#),
                96.0,
                3.0
            ),
            None
        );
        assert_eq!(
            restroke(
                &ICON.replace(r#"stroke-width="2""#, r#"stroke-width="x""#),
                96.0,
                3.0
            ),
            None
        );
        assert_eq!(restroke("no svg here", 96.0, 3.0), None);
    }
}
