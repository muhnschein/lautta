# Harbour validator warnings

`sdk-harbour-rpmvalidator` (commit `7dd7dd5`) runs on every CI build (`tools/ci/check-rpm.sh`). Errors fail build. Every warning it may print is listed here with reason; unlisted warning fails build.

- `[/usr/share/applications/harbour-lautta.desktop] X-Nemo-Application-Type should be silica-qt5 for apps importing Sailfish.Silica in QML`
- `[/usr/share/applications/harbour-lautta.desktop] X-Nemo-Application-Type should be silica-qt5 (not a Silica app?)`

  Both from `X-Nemo-Application-Type=no-invoker`. netvfs bridge serves only process running registered executable `/usr/bin/harbour-lautta`; under silica-qt5 booster it is booster's binary, bridge refuses. Cost: slower cold start.
