# Jig GPUI-CE fork patch ledger

## Candidate authority

- Upstream repository: `gpui-ce/gpui-ce`
- Selected base: `f2de912c5831d41d7aa6d9938f78969e49c7604d`
- Integration branch: `feat/runtime-accessibility-refresh`
- Companion component revision: pending synchronized integration
- Consumer revisions and adopted tag: pending proof gates

This branch is an unpublished implementation candidate. Publication was not
authorized, so the upstream references below remain required adoption gates.
Do not pin a consumer until every retained patch has a durable upstream URL or
is explicitly classified as Jig-specific, the synchronized component revision
is recorded, and the reviewed verification matrix is green.

## Retained patches

| Capability | State | Ownership seam | Evidence | Component dependency | Removal condition |
| --- | --- | --- | --- | --- | --- |
| Stable accessibility identifiers | Proposed upstream; URL required before adoption | `StatefulInteractiveElement`, `AriaProperties`, AccessKit node writing | Builder unit test plus finalized `TreeUpdate` coverage in the cached-view regression | Component controls must forward `accessibility_id`; no source-compatible no-op may remain | Remove when the selected GPUI-CE base emits an equivalent author ID with the same ID-and-role preconditions |
| Invalid accessibility state | Proposed upstream; URL required before adoption | `StatefulInteractiveElement`, `AriaProperties`, AccessKit node writing | Builder unit test covers true and omitted false; finalized two-frame update proves true-to-false clearing | Component Input validation must forward `aria_invalid` | Remove when the selected GPUI-CE base emits `Invalid::True` and clears it by omission with equivalent tests |
| Platform-less test-window handles | Proposed upstream; URL required before adoption | `TestWindow` raw window/display handle implementations | Focused tests cover both handle traits | None | Remove when both methods return `HandleError::NotSupported` upstream |
| Accessibility continuity across cached views | Proposed upstream; URL required before adoption | `ViewElement` prepaint cache reuse | Two-frame finalized-tree regression proves stable identity and invalid metadata; render counts measure the accessibility-active bypass and inactive cache reuse | Component propagation tests rely on the finalized-tree test hook | Remove when upstream either replays cached accessibility subtrees or bypasses reuse while accessibility is active with equivalent full-tree evidence |

## Historical disposition

| Historical change | Disposition |
| --- | --- |
| `d8bb3def` fork provenance | Superseded by this ledger; not ported |
| `6cb0721a` accessibility identifiers | Semantically ported on the selected base |
| `7c0cb430` uniform paint transforms | Kept out of this patch series; tracked by the independent R3 implementation |
| `e2ec12d3` safe test-window handles | Semantically ported on the selected base |
| `e2ba0360` invalid accessibility state | Semantically ported on the selected base |
| `a81cca8a` mixed clipboard entries | Omitted from the adoption candidate; remains optional and independent |

## Dependency and adoption gates

The candidate must retain the current GPUI-CE package identities (`gpui-ce`,
`gpui_ce_platform`, `gpui_ce_web`, and `gpui_ce_macros`) and use one immutable
runtime revision throughout the companion component graph. Before adoption,
record the exact component commit, target-aware positive and negative Cargo
tree proofs, pull-request CI conclusions, security audit run, and the annotated
non-moving adopted tag in this file.
