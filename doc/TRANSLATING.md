# Translating

Lautta speaks every language Sailfish OS ships; same 39 as Piirit and Salama. English is source.

## Language selection

`translations/harbour-lautta-<lang>.ts` compiles (`lrelease -idbased`, RPM `%build`) to `harbour-lautta-<lang>.qm` in `share/harbour-lautta/translations`. At start `lauttaInstallTranslations` (`crates/lautta-qt/src/lib.rs`) installs `harbour-lautta.qm` (English plurals), then catalogue for system locale (reader's *Language* setting) over it: `-pt_BR` Brazil, `-pt` Portugal, `-de` every German locale. No catalogue → English.

## Files

- `harbour-lautta.ts`: source catalogue. Every `qsTrId()` in `qml/`, English plurals filled (`tools/english-plurals.py`).
- `harbour-lautta-<lang>.ts`: one language. `language` attribute = `<lang>`; lupdate counts plural forms from it.

Never edit string list by hand. `make translations` regenerates all catalogues in one lupdate run: new string lands unfinished everywhere, removed string leaves everywhere. New language: write header to `translations/harbour-lautta-<lang>.ts`, add `<lang>` to `LANGUAGES` in `crates/lautta-qt/tests/translations.rs` and count in `tools/ci/check-rpm.sh`, run `make translations`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="<lang>"></TS>
```

## Gates

- `make translations-check` (`tools/translations-check.sh`, in `make check`): regenerating changes no catalogue; every catalogue compiles with lrelease, no warning.
- `crates/lautta-qt/tests/translations.rs`: catalogue per Sailfish OS language; each has exactly source ids, own `language`, none unfinished or empty; every translation keeps its English placeholders (`%n` only where English singular keeps it); locale installs right one, country before language; Polish plurals by Polish rule; no catalogue → English.
- `tools/ci/check-rpm.sh`: RPM ships all 39; Finnish locale on SDK target reads Finnish.

Changed English string fails build until every catalogue has it again. Cost paid 39 times. Get English right once: `doc/PROJECT.md` "Words on screen".

## Writing English

English is translators' source. Metaphor, ellipsis, ambiguous phrasal verbs don't survive. Say what thing does, plain words: literal verbs, plain verb over phrasal; sentence case; `…` not three dots; `%n` for counts. Ids say where string shows (`-menu-`, `-title`, `-header`, `-cover-`): translators read them for length and part of speech.

## Filling a catalogue

Two references, in order:

1. **Piirit's and Salama's catalogues** (`translations/piirit-<lang>.ts` in [Piirit](https://github.com/muhnschein/piirit), `translations/harbour-salama-<lang>.ts` in [Salama](https://github.com/muhnschein/salama)) for register and style: address form, button as imperative/infinitive/noun, quotes, punctuation; their word where they translate same English (Cancel, Share, Settings, Retry). German uses *du*.
2. **File-manager words** readers know from phone and desktop file managers in their language (Sailfish OS, Android Files, GNOME Files): file, folder, item, location, Trash, paste, compress, extract. One term per concept in whole catalogue. String naming another label (*Settings › Search*, *Browse*, *View options*) uses that label's translation verbatim.

*Lautta*, *netvfs*, protocol and product names (SFTP, SMB, WebDAV, Nextcloud) never translated; *Lautta* keeps Latin letters, inflects where grammar needs it (Finnish *Lauttaan*). Placeholders (`%1`, `%n`) stay; every plural form keeps `%n` where English does: Qt picks form by rule, some languages' "one" form also serves 21, 31. Short labels (menu items, headers, cover) stay short.
