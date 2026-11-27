# Verifying an RPM

Same commit, same RPM, byte for byte. Check one:

```
$ git clone https://github.com/muhnschein/lautta.git
$ cd lautta
$ tools/verify-rpm.sh --ref <commit> harbour-lautta-<version>-1.aarch64.rpm
```

`<commit>`: full hash, branch or tag the RPM was built from. CI artifact
`harbour-lautta-aarch64` of a push to `main`: that push's commit. Of a pull
request: `refs/pull/<number>/merge` while the pull request is unchanged (CI
builds it merged into `main`). Several RPMs: one run, a line each, exit is
the worst.

Needs Linux on x86_64, git, Docker, the i386 C library (Ubuntu: `libc6-i386`;
the SDK's compiler is an i486 program) and rustup. First run: SDK image pull
(~13 GB unpacked, where Docker keeps images), ~10 min build. Builds in a fresh
clone under `~/.cache/lautta-verify`, never in your tree: ~4 GB, checks for
5 GB free. Elsewhere: `LAUTTA_WORKDIR=/some/big/disk tools/verify-rpm.sh …`.

| Result | Exit | Meaning |
|---|---|---|
| `VERIFIED` | 0 | Same file as the rebuild. |
| `SAME FILES` | 1 | Every file, mode, owner inside matches; packaging (build time, file dates, host) differs. Expected for RPMs built before reproducible builds. |
| `DIFFERENT` | 2 | Files differ. Please open an issue with the output. |
| | 3 | Couldn't check (no Docker, no such commit, build failed). |

## What makes it reproducible

- Package build time and file dates: commit's time (`SOURCE_DATE_EPOCH`, set
  by `tools/ci/build-rpm.sh`); build host fixed (spec).
- Built from a copy of the tracked files without `.git`
  (`tools/ci/build-rpm.sh`): mb2 takes the version from the spec, not from
  git tags.
- Build paths: binary records none. `tools/ci/cross-build.sh` maps lifted
  SDK and cargo home to `/sdk`, `/cargo` (rustc `--remap-path-prefix`, gcc
  `-ffile-prefix-map`); cargo names the workspace's own sources relative to
  the checkout. Output folder leaves no trace either: `third_party/cpp_common`
  names rust-cpp's callback table after the crate's build folder, not its
  full path.
- Build environment: SDK image pinned by digest (`Makefile` `SDK_IMAGE`:
  cross compiler, sysroot, Qt, rpm); Rust pinned (`rust-toolchain.toml`).
  Packaged in the image at a fixed path.
- Inputs pinned: `Cargo.lock` (crates by checksum), `third_party/qttypes` and
  `third_party/cpp_common` in the tree.

CI checks it for releases: job "RPM rebuilds byte for byte" runs
`tools/verify-rpm.sh` on the RPM job's package, on another runner, from a
fresh clone at another path, with another cargo home, time zone, umask and
time. It runs only in a CI run started by hand (Actions → CI → Run workflow;
release steps require one on the release commit), not on pull requests or pushes to `main`.

## What it doesn't cover

- Sailfish SDK's compiler and libraries: upstream's image
  (`coderus/sailfishos-platform-sdk-aarch64`), trusted as by every Sailfish
  app, not rebuilt here.
- Rust toolchain: rustup's release binaries, checked by rustup, not
  bootstrapped here.
- Podman, macOS, Windows, ARM hosts: untested. Image is x86_64; its build
  user (uid 100000) is beyond rootless Podman's default subordinate ids.
