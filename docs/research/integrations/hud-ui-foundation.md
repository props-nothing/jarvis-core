# HUD UI foundation

- Status: selected for incremental implementation
- Access date: 2026-10-16
- Owners: JARVIS HUD
- Versions: React and React DOM **19.3.1** (MIT), Vite **8.3.4** (MIT), TypeScript **7.0.2** (Apache-2.0), React/React DOM types **19.3.0**. Versions are exact in package.json and transitive dependencies locked.

## Settings control follow-up

Accessed 2026-10-16: [W3C WAI-ARIA APG combobox pattern](https://www.w3.org/WAI/ARIA/apg/patterns/combobox/). The existing editable model input uses manual list selection: DOM focus stays in the input, a named listbox exposes options, expanded/active/selected state is explicit, arrows navigate, Enter accepts and Escape dismisses without changing the draft. Ordinary fixed choices remain native selects. The picker button is outside the Tab sequence; the input provides keyboard access. No new package, API, authentication or network operation is introduced. Provider model discovery retains its existing authenticated Rust route and contracts. Screen-reader behavior still needs real assistive-technology acceptance beyond DOM and keyboard checks.

## Operations and official evidence

Consulted vendor indexes first: [React llms.txt](https://react.dev/llms.txt), [Vite llms.txt](https://vite.dev/llms.txt). [React createRoot](https://react.dev/reference/react-dom/client/createRoot) supports partial-page roots and requires unmounting before another system removes the container. [Vite production builds](https://vite.dev/guide/build) supports a bundled library entry. [TypeScript handbook](https://www.typescriptlang.org/docs/handbook/intro.html) defines typed compilation. Exact published versions, engines and licenses were verified with npm against the official registry: https://registry.npmjs.org/react/19.3.1, https://registry.npmjs.org/react-dom/19.3.1, https://registry.npmjs.org/vite/8.3.4, https://registry.npmjs.org/typescript/7.0.2. Vite requires Node ^20.19.0 or >=22.12.0; development uses Node 24.

[Radix introduction](https://www.radix-ui.com/primitives/docs/overview/introduction) and [Tabs](https://www.radix-ui.com/primitives/docs/components/tabs) were evaluated. Published @radix-ui/react-tabs 1.2.1 supports React 19. No Radix dependency is needed for the catalog's native buttons; do not ship unused widgets. [Playwright accessibility testing](https://playwright.dev/docs/accessibility-testing) informs future browser automation; browser smoke and existing behavioral tests remain required, and are not an accessibility certification.

## Mapping and constraints

CI uses Node 24 via the official `actions/setup-node` v6 tag, verified with `git ls-remote https://github.com/actions/setup-node.git refs/tags/v6` on the access date and pinned to `249970729cb0ef3589644e2896645e5dc5ba9c38`. This is build infrastructure only.

- One local build-time entry; a contained React catalog mounted by the legacy settings adapter. Typed input data, text children only, no HTML insertion, no runtime CDN or dynamically fetched module.
- No external authentication, quotas, user-data transfer or provider calls. These are local build libraries, not connected services. Rust continues serving authenticated API data and enforcing policy.
- Static output included in the Rust binary. Node is a developer/CI dependency only. Preserve license notices and strict CSP. Verify rebuilt output matches the committed asset.
- Deterministic package-lock and exact versions constrain updates. React must unmount before legacy replacement. No React control owns policy, keys or OAuth redirects.
- Unresolved: full legacy migration, native screen-reader acceptance and browser-engine matrix remain separate work; do not equate typechecking the catalog with typechecking legacy scripts.
