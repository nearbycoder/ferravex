# Parent integration:213 prepared batch spawning

Reviewed monotonic total-sparse-index budgets, independent component/entity
preparation, fixed-array ownership return on reported errors and selected-free
index preflight. Legacy spawn remains growing/unbounded by prepared budgets.
No panic rollback or general allocation-free spawn guarantee is inferred.

Parent all-feature76unit tests,14runnable+5compile-fail docs, strictall-target
ECSClippy, nativeengine/facade and ECS/engine/facadeWasm checks pass. Exact213
source hash784b417f... and argv/logs are in integration-213/report.json. No
registration or dependency edits were needed.

This releases task206's bounded deferred ECS adapter after205 and213 checks.
Future214 selected-component duplication remains a separate queued convenience.
ECS identity and existing optional/filter/tuple contracts remain preserved;
game adoption and combined release/browser validation are pending.
