# Jolla Gallery's image viewer (adapted)

The image viewer (SPEC PRV-4) is Jolla Gallery's. These files are adapted
copies, BSD-3-Clause, © Jolla Ltd., Open Mobile Platform LLC and Jolla Mobile
Ltd (the licence and its copyright lines: [`BSD-3-Clause.txt`](BSD-3-Clause.txt);
each file keeps its own SPDX header):

| File | From |
|---|---|
| `FlickableImageView.qml` | [jolla-gallery](https://github.com/sailfishos/jolla-gallery) `f46dfc1` `src/pages/FlickableImageView.qml` |
| `VerticalPageBackHint.qml` | jolla-gallery `f46dfc1` `src/pages/VerticalPageBackHint.qml` |
| `GalleryImage.qml` | [sailfish-components-gallery](https://github.com/sailfishos/sailfish-components-gallery) `2247c60` `src/ImageViewer.qml` |
| `GalleryOverlay.qml` | sailfish-components-gallery `2247c60` `src/GalleryOverlay.qml` |

They are copied, not imported: `Sailfish.Gallery`, `com.jolla.gallery` and
`Sailfish.Silica.private` are not Harbour imports. What lautta changed:

- Images only: the video player, poster and seek controls, the image
  editor and Ambience are gone.
- Zooming is lautta's `../ZoomableFlickable.qml` (LGPL-2.1-or-later, the
  behaviour of Silica's private ZoomableFlickable written anew, rules in
  `../ZoomMath.js`), with a double tap besides the pinch. Its drag detector
  is private C++, so a zoomed image holds the swipes and the page's back
  navigation until it is zoomed out.
- Images turn upright themselves (`autoTransform`); remote images come
  through `image://lautta-file/`.
- The overlay's buttons are lautta's actions, signalled to the page
  (`../../../viewers/ImageViewer.qml`): details, Delete, Open with (where the
  Gallery's Edit is), Share, More; the close button is Silica's
  DismissButton's; the fades are `../FadeGradient.qml`.
- Texts are lautta's (`qsTrId("lautta-…")`), the back hint's counter is
  under `/apps/harbour-lautta/hints/`.
