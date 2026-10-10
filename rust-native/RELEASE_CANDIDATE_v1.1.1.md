# v1.1.1-rc.1 — EPIC and Meteosat same-image diagnostics
This is a **release candidate**, not a GitHub stable Release. v1.1.0 remains unchanged.

- Meteosat: query current WMS GetCapabilities, resolve the server's newest
  `mtg_fd:rgb_geocolour` TIME, and request that explicit timestamp rather
  than the layer's possibly stale default. Missing or invalid time now
  gives an actionable error instead of pretending a static picture is new.
- NASA EPIC: log the actual newest observation date returned by NASA's
  `/api/natural`. As of 2026-10-10 the official API still reports imagery
  from October 7, which software cannot artificially update.
- For both providers, compare the downloaded satellite image bytes and
  render options against the last published BMP; if they are identical,
  **do not rewrite the existing BMP**. Logs explain the unchanged photo.
- Tests: WMS time extraction, latest EPIC metadata selection, source
  fingerprints, unchanged BMP behavior, Windows release build and live
  Meteosat WMS image fetch/decoding.

No code signing. Hotplug/virtual desktop emulation is not changed.
