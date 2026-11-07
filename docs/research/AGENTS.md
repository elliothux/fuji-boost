# Research documentation

- Keep progress and conclusions in `docs/research/<domain>/YYYY-MM-DD.md`.
- Append same-day work to its domain file. Use the actual date; do not add experimental logs to root `RESEARCH.md` or this directory's `README.md`.
- Update the index when a new domain/date file is created. Link across domains instead of duplicating records.
- `research/` is committed to Git: keep only small evidence that current models or tests depend on and that will be used again. Preserve source identity and evidence limitations.
- Put temporary output, logs, traces, dumps and bulky run artifacts in `.temp/` (gitignored). Never put firmware, RAM dumps or real-camera data containing the serial number in `research/`; raw real-camera captures go to `.temp/real-camera/`.
- Earlier evidence that is no longer kept was moved to `.temp/research-archive/` (local only); old documents name those paths as plain text.
