# JARVIS HUD UI and control review

Reviewed 16 October 2026 against `fd5b736` and the initially clean working tree.

> **Historical findings, subsequently repaired:** the [implementation follow-up](#implementation-follow-up) links the fixes and revised owner-directed visual hierarchy. Use the [current settings guide](../user/settings.md) for present behavior. Preserve the observations below as evidence; do not treat them as a current unresolved defect list.

JARVIS has a coherent visual identity and useful security foundations. Its control interface has outgrown the original small, dependency-free HUD. The main problems are ambiguous integration navigation, unreliable form lifecycle, fragmented setup journeys, and a presentation that gives more space to the animated presence than to everyday work. A component library will help maintain the interface, but the information architecture and state contracts need to improve first.

This is a review and proposed implementation sequence. It does not change product behavior, adopt a framework, supersede an ADR, or mark an acceptance slice complete.

## Evidence and limits

- Read the architecture overview, product requirements, roadmap, current acceptance ledger, settings documentation, and relevant UI/control code. Inspected all 13 settings sections, the HUD and Operations in a real browser against a newly built daemon with a disposable portable profile.
- Checked the desktop layout at 1280 × 720 and mobile layout at 390 × 844. Browser interactions used a loopback review proxy that supplied only the disposable profile credential. No owner profile, account, memory, or credential was used. The normal built-in documentation/browser adapters initialized with the fresh daemon; no owner integration was connected or tested.
- Reproduced the draft-loss, focus-loss, restart-indicator, and MCP-search problems below. The mobile measurements are from the actual rendered page.
- `node --test scripts/test-hud-settings.mjs scripts/test-hud-console.mjs scripts/test-hud-memory.mjs`: **130 passed, 0 failed**. The memory-only suite has **17 passing tests**.
- `cargo test -p jarvisd --lib --locked hud::tests`: **7 passed, 0 failed**. `cargo build -p jarvisd --locked` also passed. No errors or warnings appeared in the inspected browser console.
- No production code changed, and the full Rust formatting/lint/test baseline was not rerun for this review. The checks above are focused evidence, not a workspace-wide or accessibility certification. Provider consent, real accounts, populated long-running workflows, screen readers, and native macOS/Linux behavior were not exercised.

Source locations below are repository-relative and refer to this reviewed revision.

## Findings in priority order

### 1. P1 — “It only reads” disguises an approval decision

**Observed UI and confirmed backend semantics.** Advanced → Add a program has an “It only reads” switch. Its help says Yes lets the server's tools run without asking. This is an owner assertion used to classify tools; it does not make a program incapable of writing. The primary label can be read as enabling a restriction when it actually selects a more permissive policy posture. The accessibility tree names the checkbox only “No,” omitting the purpose entirely.

Use “Approval behavior,” defaulting to “Ask before using this server.” Put an explicit “Trust this server's tools as read-only” choice in the advanced permission details, with its effect stated beside it. Prefer per-tool classification where supported. Do not imply that metadata from an untrusted MCP server proves safety. Keep ordinary approvals as one clear yes/no; this recommendation adds no approval levels or devices.

**Evidence:** `apps/jarvisd/src/hud/settings.js:1858–1866`; `apps/jarvisd/src/mcp_admin.rs:13`; `crates/jarvis-mcp/src/definition.rs:270`. This is a misleading-control finding, not evidence that Rust authorization was bypassed.

### 2. P2 — Saving one field discards other drafts and keyboard focus

**Reproduced.** In Brain, type `review-unsaved-model` into the main Model name without saving. Enter `50000` as the daily token limit and save that field. The model draft disappears and focus falls back to the document. Navigating away and back also rebuilds the panel without retaining ordinary drafts.

`saved()` reloads settings; `renderSettings()` replaces every child of the current panel. Only selected cases, such as appearance and some account polling, have draft protection. Preserving scroll position does not preserve form state.

Keep separate saved values and editable drafts keyed by section/entity. Patch only the acknowledged setting after a successful save. Retain unrelated non-secret drafts and focus across requests and navigation; provide Save/Discard at the logical form boundary. Keep secret handling deliberate and write-only, without putting drafts into browser persistence. Use a dirty-form close policy instead of silent loss.

**Evidence:** `settings.js:108–122`, `settings.js:2419–2431`, `settings.js:2525–2540`.

### 3. P2 — Reloading loses the restart requirement

**Reproduced.** Saving the daily token limit shows “Waiting for a restart: Daily token limit.” Reload the browser without restarting the daemon, open Settings, and the footer says “Nothing is waiting for a restart” with its restart button disabled.

Most pending changes live in the page's `pendingRestart` array. MCP access/programs can recover their own pending state from specialized endpoints, but general settings cannot. Changes from another browser or the CLI have the same visibility problem.

The daemon should report saved versus running configuration revisions and which settings need a restart. The UI should display that authoritative result and clear it only after observing the running revision. Derive live/restart behavior from the response contract rather than hand-maintained frontend flags. Provide a reviewable list of pending changes; retain the existing protection against interrupting active tasks.

**Evidence:** `settings.js:94–111`, `settings.js:2623–2648`; general settings response at `apps/jarvisd/src/settings_service.rs:150–164`.

### 4. P2 — Three integration locations require knowledge of implementation details

**Observed.** “Connections” mainly contains GitHub, while Google is a sibling tab. “Connect apps” means other applications connecting *into* JARVIS. Custom MCP services that JARVIS uses are under Advanced → For developers → Add a program. Browser and documentation tools are managed elsewhere under Permissions. The similar labels conceal opposite data/control directions.

Create one **Apps & tools** section for accounts and capabilities JARVIS uses. Include Google, GitHub, web search, browser, documentation, and an **Add custom MCP server** entry. Use provider/capability cards with one primary setup action. Put inbound access under **Use JARVIS elsewhere**, separate from the account catalog, and identify local clients versus internet access explicitly.

Keep MCP visible as a precise technical term in the custom setup path and searchable help. Hiding it behind “programs” does not simplify setup for someone who already has an MCP URL.

**Evidence:** `settings.js:59–79`, `settings.js:1923`, `settings.js:2080`, `settings.js:2516–2522`.

### 5. P2 — Search cannot find the settings users are looking for

**Reproduced.** Searching Settings for `mcp` yields “No settings sections match your search,” although Advanced contains MCP setup. Search matches only tab names and their short descriptions. Other actual field names and concepts, such as quiet hours or a voice ID, are absent from that index.

Index setting labels, provider names, descriptions, keywords and aliases. Results should say “Custom MCP servers → Apps & tools” or “Quiet hours → Notifications,” navigate to the precise control, and focus it without discarding drafts. A smaller immediate repair is to add aliases and rename the current control “Filter sections,” making its limited behavior honest.

**Evidence:** `settings.js:2311–2318`; `apps/jarvisd/src/hud/index.html:134–138`.

### 6. P2 — Provider setup needs a coherent save and test lifecycle

**Source inspection and observed instructions.** The model page clears a newly entered key when loading models or testing, then instructs the owner to paste it again for saving. One visible Save also performs two separate writes: provider/model configuration first, secret second. If the second request fails, the configuration has already changed, although the overall operation reports failure. This review did not inject that failure or demonstrate credential disclosure.

Offer a clear **Save connection**, then **Check connection / Choose model** flow using the saved secret. Alternatively, design a short-lived server-owned setup transaction. Commit the address/model/credential association together, or provide explicit compensation and partial-result reporting. Failed saves must leave the old usable configuration intact. Provider calls should remain explicit; the UI must not silently turn opening Settings into a paid model test.

**Evidence:** `settings.js:600–661`; independent batch/key routes in `apps/jarvisd/src/settings_service.rs`.

### 7. P2 — Mobile hides the primary action below the animated stage

**Measured.** At 390 × 844, the stage occupies about 540 px, the conversation begins around y=634, and the composer begins around y=1116. The owner must scroll before typing. On desktop, roughly two-thirds of the page goes to presence, leaving a comparatively narrow conversation column.

Default to a conversation-led layout. Keep the head as an optional expanded **Presence** view, or a compact status element beside the conversation. On mobile, place the composer in the initial viewport and make conversation/activity top-level navigation. Preserve a persistent path to pending approvals and Stop. Let the desktop split be resizable and remember the preference locally.

The mobile settings dialog also has a short, independently scrolling tab strip above an independently scrolling form. Replace it with a section picker or list/detail navigation instead of compressing 13 sections into wrapped tabs.

**Evidence:** `apps/jarvisd/src/hud/hud.css:571–618`; rendered mobile geometry.

### 8. P2 — Operations and settings mix different kinds of work

**Observed.** Operations contains approvals, active runs, projects, schedules, triggers, recent runs, contacts, memory, two sets of metrics, and a 56-tool capability list. Some empty states send the owner to the CLI. Meanwhile, background coding tasks and their evidence are inside Folders & code settings, below ten agent choices.

Make **Activity** show decisions, running work and recent outcomes. Put **Automations** and **Knowledge** in dedicated views; group projects with work. Move background coding tasks into Activity/Projects and leave agent selection in settings. Place the tool catalog in Apps & tools, with a shortcut from Activity. Keep a compact usage summary; move detailed telemetry to Diagnostics. Do not remove Memory's existing source, correction, conflict and forget safeguards.

**Evidence:** `index.html:74–127`; `settings.js:783–1078`; `hud.js:2188`.

### 9. P2 — Connection state is honest but fragmented and hard to act on

**Observed.** Much of the UI correctly warns that saved configuration is not live health. However, “set,” “off,” “configured,” “connected,” and “starts after a restart” vary by integration. An unset default can appear as OFF even when the default behavior still works. Overview omits GitHub and custom MCP servers from its accounts summary. Repeated disclaimers consume space without providing a next step.

Display separate, compact facts: **Setup**, **Sign-in**, **Tools**, and **Last check**. For example: “Signed in · Tools pending restart” with “Restart to enable tools.” Show optional, unused integrations as “Not connected,” not a warning that setup is broken. Display check timestamps and failures near the relevant account; never turn saved-key presence into a green live-health claim. Generate Overview from the same capability records as the detail screens.

**Evidence:** `settings.js:197–213`, `settings.js:231–235`, `settings.js:1754–1782`, `settings.js:2207–2268`.

### 10. P2 — Browser coverage is missing from the quality gate

**Confirmed.** CI parses the four JavaScript files and runs settings/console tests, but omits `scripts/test-hud-memory.mjs`, including its 17 current tests. Several console tests extract code by source-string boundaries, and settings tests implement a custom DOM. These are useful logic checks but do not exercise real focus, browser layout, accessible names or modal behavior. All 130 passing tests coexist with the reproduced defects above.

Add the memory suite to CI immediately. Add a small real-browser suite for draft preservation, focus after saving, restart persistence, search-to-control navigation, mobile composer visibility, permission wording/labels, and OAuth stale-response handling. Add accessibility scans and a manual keyboard pass. Keep the existing falsification/security tests; browser testing supplements them.

**Evidence:** `.github/workflows/ci.yml:42–48`; `scripts/test-hud-console.mjs:8–12`; `scripts/test-hud-settings.mjs:10–17`. [Playwright's accessibility guidance](https://playwright.dev/docs/accessibility-testing) documents an axe integration and explains why automated checks still need manual assessment.

### 11. P2 — A secondary request can make the whole assistant appear offline

**Source-confirmed path; not fault-injected.** `refresh()` loads runs, approvals, schedules and triggers with one `Promise.all`. Any failure marks the entire HUD offline. The header then disables sending even if the conversation API remains usable. Polling repeats every 1.5 seconds without an in-flight guard, so slow replies can overlap.

Track connection/session health separately from each panel's availability. Keep known stale data visibly marked, provide a local Retry, and avoid turning an optional-panel failure into a global outage. Use one polling owner with cancellation, backoff and visibility awareness; adopt an aggregate event stream only when its protocol and recovery semantics justify the change. The canvas also keeps requesting frames under overlays/reduced motion; measure idle rendering and suspend unnecessary draws.

**Evidence:** `hud.js:2442–2473`, `hud.js:2651–2662`, `hud.js:2682`.

### 12. P2 — Conversation history behaves like a cache without saying so

**Source-confirmed.** History is stored under origin-wide browser keys, limited to 30 chats and the last 60 entries in each. Storage failures are swallowed. The keys are not scoped to a profile identity, so reusing the same browser origin for another profile can display the previous profile's local transcript. This is not evidence of a cross-origin or remote data leak.

Expose whether history is saved locally and whether saving has failed. Scope any cache by daemon/profile identity, and define how it relates to canonical server sessions. Add load-older/export affordances before presenting a bounded cache as full history. Browser-local appearance can remain local; conversation ownership should follow JARVIS's durable state contract. A move to canonical history is a separately accepted storage/API slice.

**Evidence:** `hud.js:143–175`; existing product requirements for durable sessions and messages.

## Proposed navigation and setup model

Use a small number of top-level working areas: **Chat**, **Activity**, **Automations**, **Knowledge**, and **Settings**. Projects can initially be a filter/detail within Chat and Activity. These are destinations, not five simultaneous dashboards.

| Settings destination | Contents and existing controls |
| --- | --- |
| General | Appearance, density, motion, startup/recovery preferences |
| Models | Main provider/model; optional fallback and background models; usage limits |
| Apps & tools | Google, GitHub, search, browser, documentation, custom MCP servers |
| Voice & channels | Browser speech, ElevenLabs voice, Telegram, phone setup |
| Files & coding | Folder grants, coding agent selection, optional code sandbox |
| Permissions | Effective tool access, approval rules, overrides and their source |
| Notifications | Destinations, quiet hours with a time picker, test notification where supported |
| Use JARVIS elsewhere | Other AI clients, local endpoint, internet exposure and access-key management |
| Advanced | Ports, diagnostics, technical overrides and raw identifiers |

Overview should become a short “Needs attention” summary plus shortcuts, not another settings hierarchy. It should include all enabled capabilities and show optional capabilities only when useful.

The integration model should be explicit:

```text
Google / GitHub / custom MCP service → tools available to JARVIS
Telegram / phone / browser          → ways the owner talks to JARVIS
Claude Code / Cursor / other client → other applications using JARVIS
```

Use one visual connection-card pattern: name and purpose; saved account/setup; available capabilities; effective permissions; last check; primary next action; advanced details. A shared visual pattern does not mean forcing every provider into the same OAuth implementation.

For custom MCP, start with **Remote server** or **Local program**, rather than guessing from one command/address field. Remote setup asks for address, then no authentication / token / browser sign-in. Local setup asks for executable and arguments separately and shows its prerequisites. Keep issuer, scopes and client registration in a guided advanced step where they are actually required. The current pre-registered OAuth requirement must remain explicit; renaming a button cannot provide automatic app registration.

Phone setup should be a single checklist with inline dependency status and links that return to the same step: voice account → agent/number → reachability → allowed recipients → explicit test. Reference the same saved ElevenLabs connection from both speech and phone. Keep outgoing calls, incoming caller rules and ordinary speech distinct.

## UI libraries and code structure

### Current assessment

The HUD uses static HTML, CSS and four plain JavaScript scripts embedded with Rust `include_str!`. There is no frontend package manifest, component framework, TypeScript check or JavaScript lint gate in this HUD. `settings.js` is **2,652 lines**, `hud.js` **2,686**, and the four scripts total **6,600 lines**. File size alone is not the defect: the same functions own HTTP requests, mutable state, provider workflows, DOM construction and navigation.

The existing helper functions, explicit text-node rendering and dependency-free distribution are strengths. But JARVIS is already maintaining its own combobox, dialog focus manager, reactive rendering, Markdown subset, form framework and async lifecycle conventions. Different panels solve cancellation, dirty state and error handling differently. CSS contains later overrides of earlier rules, increasing the cost of understanding a change.

**Conclusion:** vanilla JavaScript was a reasonable starting point. The current arrangement is no longer a good default for an expanding settings/control application. Adding a large visual kit without changing ownership and state will not solve that.

### Recommended direction

Adopt **TypeScript + Vite + React** for the control interface, with **Radix primitives** selectively for dialogs, tabs and other complex accessible controls. Keep native inputs/selects where they are sufficient, and retain the current CSS variables and visual identity. Keep the canvas/head renderer behind a small imperative adapter with explicit start/update/dispose methods. A React component should synchronize with it, not rerender the face mesh on every animation frame.

This is a recommendation based on the interaction surface, not a measured performance result or an approved dependency selection. A one-section prototype should demonstrate smaller state complexity, correct focus, CSP compatibility and acceptable asset size before broad migration.

| Candidate | Fit for JARVIS |
| --- | --- |
| TypeScript | Typed settings/account/tool states, explicit request results and component contracts. Compile-time checking complements Rust validation; it never replaces it. |
| Vite | Build-time bundling into static assets that the daemon can embed. Node is needed for contributors/release builds, not for an installed JARVIS user. |
| React | Reusable forms and account cards with explicit state ownership; an established way to integrate the existing imperative canvas. |
| Radix primitives | Useful accessibility/interaction foundations with custom styling. Audit each chosen primitive against the existing CSP; do not weaken the policy merely to adopt a component. |
| Lit | Credible smaller alternative if native custom elements and minimal migration are the team's priority. It still requires deliberate form/request state design. |
| Plain modules + TypeScript | Viable lowest-disruption first stage. Extract ownership boundaries now, but budget for maintaining complex widgets yourself. |

Do not introduce Next.js/SSR, Redux, a desktop shell, a large theme system or multiple overlapping component kits solely for this HUD. The local daemon already serves the application. Tailwind is optional styling tooling, not the answer to the current structure and state problems. An accessible searchable model picker may need a separately evaluated component; do not assume a generic select primitive handles it.

Official sources consulted on 16 October 2026: [TypeScript handbook](https://www.typescriptlang.org/docs/handbook/intro.html), [Vite production builds](https://vite.dev/guide/build), [React integration with external systems](https://react.dev/learn/escape-hatches), [Radix introduction](https://www.radix-ui.com/primitives/docs/overview/introduction), and [Lit overview](https://lit.dev/docs/). Before implementation, select and pin exact package versions, review licenses/runtime costs, and write the integration research record required by `docs/development/external-research.md`.

### Suggested ownership boundaries

```text
ui/
  app/                  navigation, layout, startup and connection state
  api/                  typed daemon client, result/error contracts
  components/           fields, dialogs, status, connection cards
  features/
    chat/               conversation and composer
    activity/           approvals, runs and coding tasks
    automations/        schedules and triggers
    knowledge/          memory, contacts and provenance
    settings/           settings drafts, save/apply state
    integrations/       account and MCP setup views
  presence/             adapter for the existing canvas/head code
  styles/               tokens, layout and component styles
```

Treat saved settings, running settings, runtime health and temporary UI drafts as different types. Keep one request lifecycle convention with stale-response protection and cancellation. Replace source-string extraction tests with imports of these modules. Generate or validate frontend DTOs against the actual API contract; do not duplicate authorization decisions in TypeScript.

Retain Rust's ownership of credentials, grants, approval decisions, effective policy, audit and durable state. Keep same-origin production assets, strict CSP, safe text rendering, explicit provider egress and the one-click approval model. A frontend build step does not require a new daemon process or move the control plane into JavaScript.

## Delivery sequence and acceptance criteria

These are proposed slices for prioritization, not additions checked off in `TODO.md`.

1. **Repair correctness in the current UI.** Preserve unrelated drafts/focus, report pending restart from the daemon, make trust labels explicit, and add the missing memory CI suite. Reproduce each observed failure in a regression test. A reload and a second client must agree on pending changes; a server trust choice must never silently derive from server-provided annotations.
2. **Clarify navigation without changing policy.** Consolidate Apps & tools, separate inbound access, add real setting search, and introduce consistent setup/status cards. A user must find GitHub, a custom MCP URL, quiet hours and external-client access without knowing the backend architecture. Update the settings guide and the accepted navigation ADR.
3. **Make setup cohesive.** Make provider/model/secret saves transactional, unify connection status presentation and bring phone prerequisites into one journey. Test failed secret writes, missing prerequisites, cancelled OAuth, delayed replies, removal and explicit restarts. Preserve all existing fail-closed guards.
4. **Introduce the frontend foundation incrementally.** Research/pin the chosen dependencies and document the asset/build boundary. Port one contained section first, then shared forms and account cards. Verify production CSP and packaged asset serving on Windows, macOS and Linux; preserve the no-Node installed-user requirement.
5. **Rebalance the working UI.** Make Chat/composer usable immediately at mobile widths, move task monitoring out of Settings, and simplify Activity. Test 390 × 844, 768 px and desktop widths, keyboard-only use, reduced motion and populated content. Use 44 px as a touch design target; distinguish it from WCAG 2.2's 24 px minimum criterion and its exceptions ([W3C](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)).
6. **Harden lifecycle and history.** Isolate panel failures, bound polling, profile-scope caches, make persistence failures visible and define canonical history loading. Fault-inject a failing secondary endpoint while keeping chat usable; test profile changes and load-older behavior. Measure idle/network costs before selecting new streaming or rendering machinery.

For implementation slices, run the narrow behavioral/falsification tests first, then the repository's required formatting, strict clippy and workspace tests. Public API/state changes need an ADR and contract coverage. Provider authentication acceptance remains separate from local fixture success. Keep these product repairs independently reviewable rather than bundling them into a full UI rewrite.

## Implementation follow-up

The owner subsequently requested implementation. See [ADR-0225](../adr/0225-hud-controls-use-explicit-state-and-task-navigation.md), the [current settings guide](../user/settings.md), and the acceptance evidence in [TODO.md](../history/todo-owner-evidence-through-2026-10-16.md#owner-requested-hud-implementation-2026-10-16). Findings above preserve the reviewed revision; they are not a description of the repaired interface.

The owner then requested a stronger overall design and a larger face with a smaller chat panel. The [design refinement](hud-design-direction-2026-10-16.md) records the corrected hierarchy and researched design skills; its acceptance evidence is tracked separately in TODO.md.
