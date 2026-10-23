# Spawn-policy / replay design gate

Task234 received this explicit parent design before implementation. Its exact returned APIs and hashes must be checked before237/239/230 depend on it.

Legacy Arena::spawn, Match spawn/step APIs, PracticeRules and replay::Capture/World remain the original default path and bytes. The existing Respawned event includes only player ID, so it cannot record a chosen position. A new opt-in policy must not silently ride that legacy replay context.

234 may introduce explicit authority-owned per-call policy or a separate wrapper, a narrow shared Match step/respawn kernel, and a policy-aware replay capture/world with distinct schema/version and policy+arena context identity. Prefer stateless index ties; any retained policy state must be recorded in checkpoint/state. The old Match checkpoint format remains unchanged. A narrow crate-private replay Capture/context/input/event helper seam is authorized to reuse the existing Recorder and codec, avoiding a second parser/format. Wrong policy context must fail before stepping. Existing Capture/World is not the recorder for opt-in policy runs.

Static authoring uses233 and actual character Terrain clearance, including ground/query completeness.250 is Vec2-only: any XZ projection is explicitly horizontal distance, while original Vec3 positions feed clearance/LOS. No safe candidate means defer without modulo/origin fallback, accepted respawn event or life-history reset. Simultaneous decisions must follow stable ID/index policy, preserve legacy ordering when disabled and reserve occupancy when the enabled policy requires it.

237 reusable output must preserve the chosen spawn policy/result/event order and deferred-life behavior.239 weapon transitions must share that actual step seam.230/231 still own protocol/session/authority adoption. New policy replay context does not authorize a new v5 field, client-selected spawn, changed practice default or automatic game adoption.
