# Lautta

![Lautta](icons/172x172/harbour-lautta.png)

[![Security Rating](https://sonarcloud.io/api/project_badges/measure?project=muhnschein_lautta&metric=security_rating)](https://sonarcloud.io/summary/new_code?id=muhnschein_lautta)[![Reliability Rating](https://sonarcloud.io/api/project_badges/measure?project=muhnschein_lautta&metric=reliability_rating)](https://sonarcloud.io/summary/new_code?id=muhnschein_lautta)[![Coverage](https://sonarcloud.io/api/project_badges/measure?project=muhnschein_lautta&metric=coverage)](https://sonarcloud.io/summary/new_code?id=muhnschein_lautta)[![Lines of Code](https://sonarcloud.io/api/project_badges/measure?project=muhnschein_lautta&metric=ncloc)](https://sonarcloud.io/summary/new_code?id=muhnschein_lautta)

> 🤖 **This project was developed using AI.** If that provenance troubles
> you, feel free to use something else.
>
> 📱 **Modern Sailfish OS only:** Sailfish OS 5.2 or later on aarch64.
> No effort is made to accommodate older targets.

## Overview

Lautta (Finnish: ferry) is a file manager for Sailfish OS. It is two things:

- **A Jolla Store app you can use today.** Sandboxed, within the store's
  rules, no network access of its own.
- **A showcase** of what Sailfish apps could do if the platform offered
  network locations, the way iOS Files does. [netvfs](https://github.com/muhnschein/netvfs)
  is a sketch of such a platform service. With it installed, Lautta browses
  servers as if they were folders on the phone.

Lautta is a Rust program with a Silica UI, in one process. It implements no
network protocol: everything remote goes through netvfs.

## Without netvfs

What you get from the Jolla Store:

- Documents, Downloads, Pictures, Music, Videos, the Android folders,
  memory cards and USB drives.
- Copy, move, rename, compress, favourites, recents, search, and a Trash.
- Viewers for images (as in Jolla Gallery), video and music, PDFs, text and
  code with syntax colours, and Markdown.
- Thumbnails for photos, videos, PDFs and text files.
- *Save to Lautta* in the share menu.

## With netvfs

Install netvfs and its bridge, and the same app also gets:

- Every SFTP and SMB server set up in *Settings → Accounts*, and servers
  typed in by hand or found nearby.
- Copy and move between any two places, phone or server, in a queue that
  survives a restart.
- Remote media streamed in place, remote files opened in other apps.
- Memory cards and USB drives mounted and unmounted from Lautta, locked
  encrypted ones shown as such, and *Settings → Storage* one tap away.

Lautta never sees a password or a key. Accounts live in Settings; netvfs asks
once whether Lautta may use them. Without netvfs, nothing of it shows.

## Limitations

- No network protocol in the app, ever.
- Only the folders the Sailfish sandbox allows: no root, no system files.
- Nothing runs once the app is closed.
- No OpenRepos/Chum.

## Building

A device package, built in the Sailfish OS SDK image (needs Docker):

```
$ git clone https://github.com/muhnschein/lautta.git
$ cd lautta
$ export SDK_IMAGE=mirror.gcr.io/coderus/sailfishos-platform-sdk-aarch64:5.2.0.15
$ export TARGET=SailfishOS-5.2.0.15-aarch64
$ make rpm
```

Everything else needs no phone and no netvfs; a fake bridge stands in for it:

```
# Host dependencies on Ubuntu 24.04: Qt 5, lupdate, the pinned Rust.
$ ./tools/ci/install-host-deps.sh nextest

# What CI runs on the host: rustfmt, translations, clippy, tests, QML tests.
$ make check
```

## Attributions

* [Jolla](https://github.com/sailfishos), for publishing
  [jolla-gallery](https://github.com/sailfishos/jolla-gallery) under the BSD
  licence. Lautta's image viewer is adapted from it.
* [qmetaobject-rs](https://github.com/woboq/qmetaobject-rs), for bringing
  Rust and QML together.
* [hayro](https://github.com/LaurenzV/hayro), for reading PDFs in pure Rust.
* [Harbour File Browser](https://github.com/karip/harbour-file-browser), for
  showing what a file manager on Sailfish OS can do.

## License

Licensed LGPL-2.1-or-later. The files adapted from jolla-gallery are
BSD-3-Clause.

Copyright © Lautta contributors.
