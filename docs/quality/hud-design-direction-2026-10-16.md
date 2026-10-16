# HUD design refinement

The owner corrected the first implementation: JARVIS's face should dominate the main HUD; conversation should be a supporting panel. This refinement follows ADR-0225's owner-selected ordering and replaces its initial conversation-first proportions.

## Direction before implementation

JARVIS is an assistant with a visible presence. Keep the expressive mesh as the memorable element. Remove the competing marketing headline, floating metric cards and numbered conversation suggestions. Use a calm instrument panel around the face, with real state and direct controls.

Palette: canvas `#060c12`, surface `#101922`, raised surface `#172530`, border `#263742`, text `#edf3f6`, accent `#68e7e0`. Existing accent preferences and semantic warning/error colors remain available. Use the operating system's text face for controls and descriptions; Bahnschrift/Avenir for the JARVIS wordmark; monospace only for code, shortcuts and numbers. No remote fonts or assets.

Desktop layout:

```text
JARVIS / active counters                     Operations  Settings
┌────────────────────────────────────┬────────────────────┐
│                                    │ Conversation / ⋯   │
│                                    │                    │
│           LARGE FACE               │ Messages           │
│                                    │                    │
│       orbital backdrop             │                    │
│         Talk to JARVIS              │ Mic / Message / ↑  │
└────────────────────────────────────┴────────────────────┘
```

The conversation column is constrained to 350–420px, about 29% on ordinary desktops, with an inset floating panel. On narrow screens, a compact face remains above conversation when height permits; the message input stays reachable. History controls belong beside history rather than among everyday actions. Keep approval prompts visible in every layout.

Settings use sentence-case headings, readable text, aligned rows and a restrained selected-navigation treatment. The integration catalog is a list: icon, name, explanation, saved-setup status and a single named action. Status never implies a successful provider health check. Keep React/TypeScript for this existing boundary and native controls elsewhere; this change needs no additional UI dependency.

Before building, checked the plan against the brief: rejected another card dashboard and a larger text hero. The face supplies the identity. All remaining visual treatment supports navigation, task status or direct input.

## Design guidance reviewed

Accessed 2026-10-16. Read instructions directly; no global skill installation or remote scripts executed.

- [Anthropic frontend-design](https://skills.sh/anthropics/skills/frontend-design), [official source](https://raw.githubusercontent.com/anthropics/skills/main/skills/frontend-design/SKILL.md). Directory reported 993.9K installs and 181.4K repository stars. Applied its subject-led hierarchy, limited typography and screenshot critique process.
- [Vercel web-design-guidelines](https://skills.sh/vercel-labs/agent-skills/web-design-guidelines), [official guidelines](https://raw.githubusercontent.com/vercel-labs/web-interface-guidelines/main/command.md). Directory reported 743.9K installs and 32.5K repository stars. Applied native controls, focus visibility, reduced motion, content wrapping and viewport checks. Popularity is a discovery signal, not proof of correctness.

Optional installation commands, if these should become permanent agent skills: `npx skills add anthropics/skills --skill frontend-design` and `npx skills add vercel-labs/agent-skills --skill web-design-guidelines`.

## Screenshot critique and acceptance

The first preview exposed two issues: the caption touched the activity divider, and the mobile settings footer used three rows. Adjusted the face/caption clearance and reduced the footer to two rows. History's successful retention notice now appears with History; failures remain visible on the main view. A long synthetic approval still leaves the mobile composer accessible.

Populated conversation checks covered six viewports from 320px to 1280px, including a landscape phone. Code blocks scroll inside the conversation. The desktop stage occupies 72% at 1280px, compared with 38% before this refinement. Browser keyboard checks confirmed Escape returns to Settings and `/` focuses Message. Integration rows open the intended existing setup page. Browser logs showed no warnings/errors in the tested flows.

Verification and remaining limits are recorded in [TODO.md](../../TODO.md). Screenshot evidence lives in `target/hud-design-*.png`; browser API data and responses are synthetic, not a live-provider acceptance claim.

## Compact cinematic follow-up

The owner identified the tall textarea, persistent migration/readiness copy and large footer as the remaining distractions. Replaced the textarea with a native single-line input inside a 48px composer. Enter submits once, composition input does not submit, and unavailable submissions retain the draft. Voice preferences moved into a native disclosure in the conversation header, with Escape/focus return and outside-click dismissal; voice failures remain visible beneath the composer. Removed the old migration banner without importing or deleting old history. Retention information stays with History, and storage errors remain visible.

Removed the desktop activity rail and redundant stage actions. Real run/decision counts appear in the header only when needed and lead directly to Activity. A single voice action sits below the face. Reduced the reserved canvas footer from 184px to 100px, added two subtle perspective orbits and inset the conversation panel. Orbit motion respects the existing reduced-motion setting; Quiet omits the decoration. The mobile active-state header shares its second row with Stop, and Compact density now produces a smaller header than Comfortable.

Browser checks measured the composer at 48px across all six existing viewport sizes, with no document overflow and the input always inside the viewport. A synthetic long-path approval remained actionable at 390px and 320px; voice preferences fit the phone layout. Verified Enter submission, history disclosure, voice dismissal/focus return and counters returning from Knowledge to Activity. New screenshots and measurements are in `target/hud-compact-*.png` and `target/hud-compact-viewports.json`. Required checks and release-build evidence are recorded in TODO.md.

## Settings consistency follow-up

The owner identified inconsistent tabs, labels, inputs and dropdowns. The source contained conflicting base and later polish rules, while the model input competed with four actions in the same grid row. Consolidated the shared form styles and responsive rules: neutral labels, one text face, 40px desktop fields, 44px phone fields, matching borders/radii, aligned help and consistent action spacing. Native selects retain native interaction. Monospace remains for code snippets and technical identifiers. Model actions now sit in a separate card footer; the optional key field is hidden for providers that need no key. The main-provider introduction is shorter and key-handling information stays beside the key field.

The editable model picker now exposes combobox/listbox/option semantics, active and selected state, and keyboard navigation. It stays closed until suggestions have been loaded, preserves arbitrary typed names, dismisses on Escape or outside focus, and closes before its form is rebuilt. One shared dismissal listener replaces listeners added on every render. Selecting a model bubbles a change so its unsaved draft survives navigation. Immediate appearance preferences no longer enter the unsaved-draft collection. Missing names were added to Telegram, coding-command and advanced connection fields; no provider, credential or approval contract changed. The [UI foundation research](../research/integrations/hud-ui-foundation.md#settings-control-follow-up) records the W3C reference and assistive-technology limits.

The synthetic preview uses all 41 public setting definitions from the Rust schema, not owner configuration. Every section was inspected at desktop and phone/tablet sizes, including expanded GitHub, coding, Telegram and external-access forms. Measurements and screenshots are recorded under `target/settings-consistency-*`; the evidence ledger in TODO.md records final checks. Existing React catalog boundaries remain unchanged; this is a consolidation of the current form adapters, not a claim that the entire settings implementation has migrated to React.
