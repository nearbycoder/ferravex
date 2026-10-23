# Journey400 release stopping point

The user stopped new implementation after83 of200 additional numbered tasks. All83
implementation tasks have returned;117 planned tasks are deferred until explicitly
resumed. A returned task is distinct from adoption in every demo or complete
platform validation. No deferred game feature is claimed delivered.

The original Journey200 batch and preserved Journey50 work are included. All15
example hosts retain their authored gameplay and their documented rendering paths.
New library APIs have runnable recipes and targeted native checks; combined
release verification is recorded below before merge.

## Returned Journey400 tasks

201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 237, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282, 283, 284, 285, 286, 287, 288, 289, 290, 291, 292, 293, 294, 295, 296, 297, 311, 312, 313, 314, 315, 316, 317, 321.

## Deferred Journey400 tasks

235, 236, 238, 252, 253, 254, 255, 256, 257, 258, 259, 260, 261, 262, 263, 264, 265, 266, 267, 268, 269, 270, 298, 299, 300, 301, 302, 303, 304, 305, 306, 307, 308, 309, 310, 318, 319, 320, 322, 323, 324, 325, 326, 327, 328, 329, 330, 331, 332, 333, 334, 335, 336, 337, 338, 339, 340, 341, 342, 343, 344, 345, 346, 347, 348, 349, 350, 351, 352, 353, 354, 355, 356, 357, 358, 359, 360, 361, 362, 363, 364, 365, 366, 367, 368, 369, 370, 371, 372, 373, 374, 375, 376, 377, 378, 379, 380, 381, 382, 383, 384, 385, 386, 387, 388, 389, 390, 391, 392, 393, 394, 395, 396, 397, 398, 399, 400.

See [the roadmap](JOURNEY_400.md), [portable plan](journey400/plan.json),
[task handoffs](journey400/receipts/), and [coding-agent API map](AGENT_GUIDE.md).

## Validation and limits

Combined workspace checks passed4096 Rust tests/doctests (14 platform/device tests
remain ignored), strict Clippy, formatting, all-target/all-feature compilation
and the first-party dependency audit. Source-host and Python/tooling suites pass.
All15 release Wasm games and the generated site build pass. The first artifact
run passed99 checks and caught one obsolete Controls click coordinate; its
replacement keyboard-path regression now passes for all15 games, including
renderer changes, saved settings and URL overrides. A final artifact matrix and
remote CI/deployment verification are recorded by the release PR. Local native
window/GPU checks require Xvfb, which is unavailable here; CI provides it. Detailed implementation receipts preserve
the commands/results actually executed. Game adoption is limited to implemented
consumers; library compilation is not browser gameplay evidence. Physical devices,
audio listening, native hardware GPU and sustained-performance claims are not
added by this release. Previously reproduced tall-scrolled WebGL capture and
native-fullscreen screenshot discrepancies remain documented validation limits.

## Resuming

Resume only the deferred IDs after a new user instruction. Read the portable plan,
owned paths, dependency receipts and actual API before changing a consumer. Preserve
compatible defaults and formats, finish accepted-event/input/tick composition,
then rebuild and verify the real demo artifact.
