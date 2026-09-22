# ADR 0002: History-mode frontend router

- **Status:** Accepted
- **Date:** 2026-09-22
- **Decision:** Use `@keenmate/svelte-spa-router` 5.3.0 in history mode.

## Context

The Tauri window hosts one Svelte 5 SPA. Epic 1 needs clean internal deep
links, route parameters, and normal browser/WebView Back and Forward behavior;
future screens must not register routes before they have a real screen. A
router also needs to work without a server rewrite because the Tauri WebView
loads one local document.

## Decision

Pin `@keenmate/svelte-spa-router` to `5.3.0`. Configure it with
`setHashRoutingEnabled(false)` and `setBasePath('/')` before Svelte `mount()`.
The route table is typed with `defineRoutes()` and currently registers only:

- `/onboarding`
- `/home`
- `/settings/:group`

Unknown paths are replaced with `/home`. Links use the router's history-aware
`link` action, while the browser's `popstate` path handles Back and Forward.

## Why this package

Version 5.3.0 is built on Svelte 5 runes, supports history and hash modes,
route parameters, typed route definitions, and a navigation API. It therefore
fits the Svelte 5/Tauri WebView runtime without introducing a UI component
library or a server runtime. The exact pin keeps route behavior reproducible;
any router upgrade must repeat deep-link, parameter, and Back/Forward tests.

## Consequences

History mode makes clean URLs available to the WebView and keeps navigation
semantics familiar. Tauri owns the document, so this does not require a server
rewrite. The catch-all behavior lives at the application boundary and safely
redirects unknown routes instead of rendering a blank screen. New routes must
be added deliberately to `src/lib/router.ts` only when their screen is in
scope.
