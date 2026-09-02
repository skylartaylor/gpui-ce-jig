# Jig GPUI-CE fork patch ledger

## Candidate authority

- Upstream repository: `gpui-ce/gpui-ce`
- Selected base: `f2de912c5831d41d7aa6d9938f78969e49c7604d`
- Published candidate branch: `jig/runtime-refresh-2026-09`
- Companion component revision: `6a27bbc0b1ed912d99c7a2049f27a183800080cb`
- Companion adopted tag: `jig-adopted/component-2026-09-01.3`
- Companion adopted tag object: `bc5b8080812da1fb40de2d68dc4cd20fb0e5558c`
- Companion reviewed candidate: `601755c2dc4657c41829e2b9f6194682e2304940`
- Companion reviewed and adopted tree: `63c5f92d624276fd8950ab5f2aedf7e0c4a1306a`
- Reviewed paired runtime code head: `781c86514dff3e667b672047bb76cef440d3f22e`
- Consumer revisions and runtime adopted tag: pending exact-head proof gates

The component fork is adopted at the immutable revision and tag above. The
runtime candidate is published for integration and review but is not yet
adopted. Do not pin a consumer until the final runtime pull request, exact-head
CI, security audit, and consumer proof gates are green and the runtime receives
its own non-moving adopted tag. Runtime release tags do not publish component
packages; the component remains a separately adopted Git dependency.

## Retained patches

| Capability | State | Ownership seam | Evidence | Component dependency | Removal condition |
| --- | --- | --- | --- | --- | --- |
| Stable accessibility identifiers | Proposed upstream; URL required before adoption | `StatefulInteractiveElement`, `AriaProperties`, AccessKit node writing | Builder unit test plus finalized `TreeUpdate` coverage in the cached-view regression | Component controls must forward `accessibility_id`; no source-compatible no-op may remain | Remove when the selected GPUI-CE base emits an equivalent author ID with the same ID-and-role preconditions |
| Invalid accessibility state | Proposed upstream; URL required before adoption | `StatefulInteractiveElement`, `AriaProperties`, AccessKit node writing | Builder unit test covers true and omitted false; finalized two-frame update proves true-to-false clearing | Component Input validation must forward `aria_invalid` | Remove when the selected GPUI-CE base emits `Invalid::True` and clears it by omission with equivalent tests |
| Platform-less test-window handles | Proposed upstream; URL required before adoption | `TestWindow` raw window/display handle implementations | Focused tests cover both handle traits | None | Remove when both methods return `HandleError::NotSupported` upstream |
| Accessibility continuity across cached views | Proposed upstream; URL required before adoption | `ViewElement` prepaint cache reuse | Two-frame finalized-tree regression proves stable identity and invalid metadata; render counts measure the accessibility-active bypass and inactive cache reuse | Component propagation tests rely on the finalized-tree test hook | Remove when upstream either replays cached accessibility subtrees or bypasses reuse while accessibility is active with equivalent full-tree evidence |
| Layout-stable uniform paint transforms | Jig-specific until proposed upstream | Window paint conversion, scene geometry, deferred draws, and view/deferred paint-cache identity | Focused matrix covers composition, unwind restoration, primitives, clips, filters, sprite origins/extents, deferred replay, cache invalidation, hitboxes, and layout-space accessibility bounds | Jig's `lift()` adapter requires the public transform scope and value API | Remove only when upstream provides equivalent paint-only uniform transforms across every covered path and Jig's unchanged consumer matrix passes |
| Spring integration primitives | Proposed upstream; URL required before adoption | Pure animation state in `spring.rs` and GPUI exports | Analytic integration, target convergence, and target-type tests | Current Longbridge component motion APIs import `SpringConfig`, `SpringState`, and `SpringTarget` | Remove when the selected GPUI-CE base exports source- and behavior-equivalent spring primitives used by the synchronized component revision |
| Synchronized repeating animations | Proposed upstream; URL required before adoption | App-owned animation epoch and repeating animation phase calculation | Focused tests cover common phase for late mounts and long-uptime precision | Current Longbridge shimmer requires `Animation::repeat_synced()` so independently mounted elements share phase | Remove when the selected GPUI-CE base exposes equivalent synchronized repeat semantics and the component shimmer path passes unchanged |
| Opt-in native HTTP client | Jig-specific until proposed upstream | `gpui_ce_platform::NativeHttpClient` and explicit `Application::with_http_client` installation | Local-server tests cover status/body preservation, non-success responses, redirect modes, transport errors, persistent connection reuse, and process crypto-provider installation/preservation; CI proves the all-target platform graph selects Ring without AWS-LC or CMake | Remote component images require the consumer to opt into a native client; `application()` remains network-disabled by default | Remove when upstream provides an equivalent opt-in native client with system proxy and TLS support without changing default application authority |

Ring avoids a second native TLS backend and the AWS-LC CMake toolchain in Jig
consumers. Unlike Rustls's AWS-LC default, Ring does not enable the hybrid
X25519MLKEM768 post-quantum key exchange; revisit this tradeoff when the
consumer toolchain can adopt one provider consistently.

## Historical disposition

| Historical change | Disposition |
| --- | --- |
| `d8bb3def` fork provenance | Superseded by this ledger; not ported |
| `6cb0721a` accessibility identifiers | Semantically ported on the selected base |
| `7c0cb430` uniform paint transforms | Semantically redesigned and ported on the selected base; the old patch was not replayed |
| `e2ec12d3` safe test-window handles | Semantically ported on the selected base |
| `e2ba0360` invalid accessibility state | Semantically ported on the selected base |
| `a81cca8a` mixed clipboard entries | Omitted from the adoption candidate; remains optional and independent |

## Dependency and adoption gates

The candidate retains the current GPUI-CE package identities (`gpui-ce`,
`gpui_ce_platform`, `gpui_ce_web`, and `gpui_ce_macros`). The adopted component
revision is pinned as this repository's `crates/gpui_ce_components` gitlink and
was reviewed against runtime code head `781c86514dff3e667b672047bb76cef440d3f22e`.
Runtime adoption still requires target-aware positive and negative Cargo tree
proofs, exact-head pull-request CI conclusions, the security audit, consumer
revision proofs, and an annotated non-moving runtime tag recorded here.
