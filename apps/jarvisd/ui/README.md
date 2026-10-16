# HUD controls

Node 24 is used for development; installed JARVIS needs only its Rust binary. Run `npm ci`, `npm run build` and `npm test` here. Commit the lockfile and `../src/hud/generated/controls.js` with source changes. CI rebuilds and checks for drift.

The React/TypeScript integration catalog is the first migration boundary. It receives ordinary typed data and a navigation callback, never a credential or policy service. Legacy `settings.js` owns the surrounding dialog and unmounts the React root before replacement. Handwritten scripts retain their strict DOM-safety tests; the vendor bundle is checked through CSP, text-rendering tests and its exact static route.

Use native controls and the existing CSS tokens. Add an accessible primitive library only for a widget that needs it. See [research](../../../docs/research/integrations/hud-ui-foundation.md) and [ADR-0225](../../../docs/adr/0225-hud-controls-use-explicit-state-and-task-navigation.md).
