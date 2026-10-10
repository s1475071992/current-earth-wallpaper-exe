# Current Earth Wallpaper v1.1.2 — discreet Obs / Upd timestamp

Windows x64 native Rust EXE (unsigned).

## Wallpaper watermark
- The bottom-right watermark is a single line, flush to the lower/right edges, using a small 5×7 bitmap font at 1–2× pixel scale and a 75% white blend (no border or opaque panel).
- Format: \`Obs 2026-10-10 18:50 UTC+8 Upd 2026-10-10 19:03:18 UTC+8\`.
- Both timestamps display in **UTC+8**. \`Obs\` comes from the image's selected observation metadata; \`Upd\` is the actual local BMP generation time.
- Meteosat (MTG): timestamp of the successfully downloaded WMS TIME, including any fallback slot. NASA EPIC: date from its observation metadata. Himawari-9: timestamp from NICT's latest.json that selects the downloaded tiles.
- GOES-East/GOES-West and FY4B: direct JPEG image endpoints do not currently provide a verified acquisition timestamp in this application; the watermark shows \`Obs --\` rather than misrepresenting a file Last-Modified or download time as acquisition.
- In a scheduled cycle, repeated satellite sources reuse the same raw image and its original observation metadata, while each desktop/monitor gets its own individually rendered BMP.
- Existing per-user watermark toggle is respected. For a watermark on every wallpaper, enable **Obs / Upd watermark** in settings.

## Bug fixes and safeguards
- Retains the v1.1.1 Meteosat WMS geostationary projection and TIME fallback fix.
- Fingerprints for NASA EPIC and Meteosat now include the updated watermark format so previously cached wallpapers are re-rendered once even when source pixels have not changed.
- Adds unit tests for UTC+8 conversion, month/year rollovers, unknown timestamp display and pixel opacity/edge alignment.
- Keeps virtual desktop and multi-monitor independent wallpaper storage.
