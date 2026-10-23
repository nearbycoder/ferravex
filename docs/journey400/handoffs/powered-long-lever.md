# Observed powered spatial hinge limit

Task223 discovered an off-center powered spatial-hinge drift limit while testing new world-frame authoring. Parent independently reproduced it on frozen223 sources and compared the helper with direct, manually authored local anchors/axes/reference and the unchanged existing insertion path. The sampled descriptors and subsequent61-tick results are identical.

Exact executable diagnostic and commands: `../integration-223/powered-diagnostic.rs`, `powered-diagnostic.py`, `powered-report.json`. At dt1/120, target1rad/s,max torque1000, fixed support at(1,2,3), dynamic0.2-radius ball at(-2,3,5), rotated parents and world axis normalized(-2,3,4):

- Pivot at rotor origin: axis error0.0000001074538, anchor separation0 after61ticks.
- Pivot at(2,-1,3): axis error0.018339662, anchor separation0.32229516 after61ticks.

The direct old local-frame route and new helper match exactly in each case. This validates equivalence of the authoring conversion; it does not establish solver stability for the long-lever powered fixture. No solver repair or changed default is claimed. Keep this concrete limitation open when considering powered off-center assemblies/lab mechanisms. Any repair should target the shared solver with this reproduction and preserve existing contacts/joints/default authored demos; it must not merely remove the difficult fixture or label it fixed through the new helper.
