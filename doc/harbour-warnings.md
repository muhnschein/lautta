# Harbour validator warnings

`sdk-harbour-rpmvalidator` (commit `7dd7dd5`, SPEC §3.2) runs on every CI build
(`tools/ci/check-rpm.sh`, TST-7). Errors fail the build. Every warning it may print is
listed here with the reason it is accepted; an unlisted warning fails the build.

- `X-Nemo-Application-Type should be silica-qt5 for apps importing Sailfish.Silica in QML`
  — the app starts with `no-invoker` (SPEC HBR-4). The netvfs bridge checks that the
  connecting process runs the registered executable `/usr/bin/harbour-lautta`
  (netvfs SPEC-v2 XB-5); under the silica-qt5 booster it would be the booster's binary
  and the bridge would refuse the connection. The cost is a slower cold start.
