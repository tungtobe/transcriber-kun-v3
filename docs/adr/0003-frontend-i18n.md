# ADR 0003: Static typed frontend i18n module

- Status: Accepted
- Date: 2026-09-22

## Context

trans-kun needs equal offline `vi`, `en`, and `ja` catalogs, immediate Svelte 5 updates, system-locale fallback, persisted preference, and a CI key-parity gate. The current product surface uses flat text keys and simple named interpolation; it does not need runtime catalog loading, rich HTML, namespaces, ICU formatting, or plural rules yet.

## Decision

Use a small Svelte-runes module with statically imported JSON catalogs. Keys follow `<screen>.<block>.<label>`, English is the defensive lookup fallback, and catalog parity is checked independently in CI. The Rust settings service remains the durable source for `uiLanguage`; local storage is only a first-render hint. Text interpolation writes ordinary Svelte text and never injects HTML.

`i18next` 26.0.2 was evaluated. Its bundled-resource initialization, `fallbackLng`, `changeLanguage`, and `languageChanged` APIs meet the runtime requirements, but its broader loading, namespace, plural, and formatting surface is not needed for the current flat catalog. Avoiding that dependency keeps startup synchronous and the offline surface small. Revisit this ADR if a shipped story requires plural rules or locale-aware formatting that cannot stay in focused helpers.

## Consequences

- All three catalogs ship in the application bundle and never require network access.
- Adding a UI string requires the same valid key in all three locale files.
- Components render translations through the reactive module; direct DOM translation and `innerHTML` are forbidden.
- Future stories extend the catalog only for strings they actually deliver.
