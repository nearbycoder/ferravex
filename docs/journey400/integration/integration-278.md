# Journey400 278 parent integration

Reviewed the registered renderer adapter, projection arithmetic, stable slot and
hidden-prefix policy, existing touch validation reuse and saved customization.
No additional repair or registration was needed. Visible regions remain absolute
logical geometry; source control channels and saved indexes are preserved. Hidden
middle slots explicitly fail rather than compact saved identities. Consumers
install authored controls, then resolve settings for drawing without reinstalling
the resolved result. Resizes cancel captures only when the rebuilt layout is
installed; arbitrary custom edits can still overlap under the legacy policy.

Parent checks passed, exact commands in `integration-278/report.json`:

- Eleven geometry, real-journal and allocation regressions with all features.
- Thirty-one existing input unit tests with all features.
- Three executable public doctests and strict scoped renderer Clippy.
- Renderer/facade all-feature Wasm compilation.
- Independently compiled and executed the complete receipt composition recipe:
  `integration-278/recipe-parent/report.json`, one binary passed. It composes HUD
  reservations, checked canvas fitting, saved customization, painting and actual
  tick-consumed PlayerInput events.
- Scoped formatting/whitespace and API-map links passed.

Source hashes: `integration-278/source-hashes.json`. Parent game adoption remains
separate for Emberline, Neon and Pocket; their accepted originalJourney200 visual
fixes are preserved. New Controls292 work can consume this stable description
without changing the touch router or save format. No fresh browser/device/GPU,
physical touch, audio/performance or whole-journey integration claim.
