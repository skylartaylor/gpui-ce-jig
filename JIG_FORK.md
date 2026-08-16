# Jig GPUI-CE fork

This repository is the GPUI framework dependency used by Jig Pro Desktop.
Its permanent branch is based on upstream GPUI-CE commit
`c30a93f54b279e39b030919982777164297dd6e3` from
<https://github.com/gpui-ce/gpui-ce>.

The fork carries only two general-purpose capabilities and their tests:

- author-provided accessibility identifiers exposed through AccessKit; and
- uniform paint-time transforms used by Jig's `lift()` presentation helper.

No Jig product state, protocol types, theme values, or application callbacks
belong in this repository.

## Commit stack

Keep the permanent changes as a short linear stack above the named upstream
base:

1. this provenance document;
2. the accessibility identifier API and focused unit tests;
3. the paint-transform implementation and its regression suite.

Cargo consumers pin the final commit SHA. Tags are descriptive only and must
never replace an immutable revision in a consuming manifest.

## Updating the base

1. Fetch `https://github.com/gpui-ce/gpui-ce` and name the candidate commit.
2. Compare its GPUI API and behavior with the GPUI revision targeted by the
   pinned GPUI Component commit.
3. Replay the linear fork stack onto a temporary branch.
4. Run formatting, clippy, unit tests, test-support tests, and release builds on
   macOS and Linux.
5. Run the consuming Jig Pro Desktop dependency, semantic, and native macOS
   verification matrix before changing its Cargo pin.

Never update a moving branch in a consuming application. A new fork revision
is accepted only after the exact candidate commit passes the complete matrix.

## Rollback

Before Jig product code consumes the accessibility APIs, its dependency commit
can be reverted to the previous `v0.2.2-paint-transform-2` source. Once product
code uses these APIs, rollback requires reverting the complete Jig feature as
well as its manifest and lockfile changes.
