use crate::{Bounds, IsZero, Pixels, Point, ScaledPixels, TransformationMatrix, Window, point};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

/// A uniform positive scale and translation applied to painted output.
///
/// The transform is baked into scene geometry without changing layout,
/// hit-testing, or accessibility geometry. Translation is applied after scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintTransform {
    /// The uniform positive scale factor.
    pub scale: f32,
    /// Translation in logical pixels, applied after scaling.
    pub translation: Point<Pixels>,
}

impl Default for PaintTransform {
    fn default() -> Self {
        Self::identity()
    }
}

impl PaintTransform {
    /// The transform that leaves painted output unchanged.
    pub fn identity() -> Self {
        Self {
            scale: 1.0,
            translation: Point::default(),
        }
    }

    /// A uniform positive scale about the window origin.
    #[track_caller]
    pub fn scale(scale: f32) -> Self {
        assert_valid_scale(scale);
        Self {
            scale,
            translation: Point::default(),
        }
    }

    /// A translation in logical pixels.
    #[track_caller]
    pub fn translate(translation: Point<Pixels>) -> Self {
        let transform = Self {
            scale: 1.0,
            translation,
        };
        transform.assert_valid();
        transform
    }

    /// A uniform positive scale around a fixed point.
    #[track_caller]
    pub fn scale_about(scale: f32, origin: Point<Pixels>) -> Self {
        assert_valid_scale(scale);
        let transform = Self {
            scale,
            translation: origin - origin * scale,
        };
        transform.assert_valid();
        transform
    }

    /// Whether this transform leaves painted output unchanged.
    pub fn is_identity(&self) -> bool {
        self.scale == 1.0 && self.translation.is_zero()
    }

    /// Returns the transform that applies `inner` first and then `self`.
    #[track_caller]
    pub fn compose(self, inner: Self) -> Self {
        self.assert_valid();
        inner.assert_valid();
        let transform = Self {
            scale: self.scale * inner.scale,
            translation: inner.translation * self.scale + self.translation,
        };
        transform.assert_valid();
        transform
    }

    /// Maps a logical point into painted coordinates.
    pub fn map_point(&self, point: Point<Pixels>) -> Point<Pixels> {
        point * self.scale + self.translation
    }

    /// Maps a painted point back into logical coordinates.
    pub fn inverse_map_point(&self, point: Point<Pixels>) -> Point<Pixels> {
        (point - self.translation) * self.scale.recip()
    }

    /// Maps logical bounds into painted coordinates.
    pub fn map_bounds(&self, mut bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        bounds *= self.scale;
        bounds + self.translation
    }

    /// Maps painted bounds back into logical coordinates.
    pub fn inverse_map_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let mut bounds = bounds - self.translation;
        bounds *= self.scale.recip();
        bounds
    }

    #[inline]
    pub(crate) fn device_translation(&self, scale_factor: f32) -> Point<ScaledPixels> {
        point(
            ScaledPixels(self.translation.x.0 * scale_factor),
            ScaledPixels(self.translation.y.0 * scale_factor),
        )
    }

    #[inline]
    pub(crate) fn map_device_bounds(
        &self,
        mut bounds: Bounds<ScaledPixels>,
        scale_factor: f32,
    ) -> Bounds<ScaledPixels> {
        bounds *= self.scale;
        bounds + self.device_translation(scale_factor)
    }

    #[inline]
    pub(crate) fn conjugate_device_matrix(
        &self,
        matrix: TransformationMatrix,
        scale_factor: f32,
    ) -> TransformationMatrix {
        if self.is_identity() {
            return matrix;
        }

        let outer = self.to_device_matrix(scale_factor);
        let translation = self.device_translation(scale_factor);
        let inverse = TransformationMatrix {
            rotation_scale: [[self.scale.recip(), 0.0], [0.0, self.scale.recip()]],
            translation: [
                -translation.x.0 * self.scale.recip(),
                -translation.y.0 * self.scale.recip(),
            ],
        };
        outer.compose(matrix).compose(inverse)
    }

    #[inline]
    fn to_device_matrix(&self, scale_factor: f32) -> TransformationMatrix {
        let translation = self.device_translation(scale_factor);
        TransformationMatrix {
            rotation_scale: [[self.scale, 0.0], [0.0, self.scale]],
            translation: [translation.x.0, translation.y.0],
        }
    }

    #[inline]
    #[track_caller]
    fn assert_valid(&self) {
        assert_valid_scale(self.scale);
        assert!(
            self.translation.x.0.is_finite() && self.translation.y.0.is_finite(),
            "a paint transform's translation must be finite, got {:?}",
            self.translation
        );
    }
}

#[inline]
#[track_caller]
fn assert_valid_scale(scale: f32) {
    assert!(
        scale.is_finite() && scale > 0.0,
        "a paint transform's scale must be finite and positive, got {scale}"
    );
}

impl Window {
    /// Composes `transform` with the current paint transform while `f` runs.
    ///
    /// This affects painted output only. Layout, hitboxes, and accessibility
    /// bounds remain in their original logical coordinate space. A matching
    /// [`Self::with_expected_paint_transform`] scope during prepaint lets
    /// deferred descendants and cached views capture the same transform.
    pub fn with_paint_transform<R>(
        &mut self,
        transform: PaintTransform,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.invalidator.debug_assert_paint();
        self.with_composed_paint_transform(transform, true, f)
    }

    /// Declares during prepaint which transform will wrap the same subtree in paint.
    ///
    /// The declaration does not transform prepaint output, so hitboxes and
    /// accessibility bounds remain in layout space. It supplies cache identity
    /// and captures the transform for deferred descendants.
    pub fn with_expected_paint_transform<R>(
        &mut self,
        transform: PaintTransform,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.invalidator.debug_assert_prepaint();
        self.with_composed_paint_transform(transform, false, f)
    }

    /// Returns the transform active for paint, or declared for prepaint.
    pub fn paint_transform(&self) -> PaintTransform {
        self.paint_transform
    }

    #[inline]
    pub(crate) fn paint_transform_folded(&self) -> (f32, Point<ScaledPixels>) {
        let scale_factor = self.scale_factor();
        (
            scale_factor * self.paint_transform.scale,
            self.paint_transform.device_translation(scale_factor),
        )
    }

    pub(crate) fn with_absolute_paint_transform<R>(
        &mut self,
        transform: PaintTransform,
        paint: bool,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        transform.assert_valid();
        let previous = self.paint_transform;
        self.paint_transform = PaintTransform::identity();
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.with_composed_paint_transform(transform, paint, f)
        }));
        self.paint_transform = previous;
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    }

    fn with_composed_paint_transform<R>(
        &mut self,
        transform: PaintTransform,
        adjust_content_mask: bool,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        transform.assert_valid();
        if transform.is_identity() {
            return f(self);
        }

        let previous = self.paint_transform;
        self.paint_transform = previous.compose(transform);
        if adjust_content_mask {
            let mask = crate::ContentMask {
                bounds: transform.inverse_map_bounds(self.content_mask().bounds),
            };
            self.content_mask_stack.push(mask);
        }

        let result = catch_unwind(AssertUnwindSafe(|| f(self)));

        if adjust_content_mask {
            self.content_mask_stack.pop();
        }
        self.paint_transform = previous;
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        self as gpui, AnyElement, AnyView, App, AppContext as _, Bounds, BoxShadow, Context,
        Corners, DevicePixels, DrawPhase, Drawable, Edges, Element, ElementId, Filter, Font,
        FontId, FontRun, GlobalElementId, GlyphId, HitboxBehavior, InspectorElementId, IntoElement,
        LayoutId, LineLayout, NoopTextSystem, ParentElement, PlatformTextSystem, Render,
        RenderGlyphParams, Size, StyleRefinement, Styled, TestAppContext, TestDispatcher,
        TextRenderingMode, UnderlineStyle, VisualTestContext, anchored, canvas, deferred, div,
        fill, hsla, point, px, scene::Scene, size,
    };
    use anyhow::Result;
    use std::{borrow::Cow, cell::Cell, panic::AssertUnwindSafe, rc::Rc, sync::Arc};

    struct Transformed {
        transform: PaintTransform,
        declare: bool,
        child: AnyElement,
    }

    fn transformed(transform: PaintTransform, child: impl IntoElement) -> Transformed {
        Transformed {
            transform,
            declare: false,
            child: child.into_any_element(),
        }
    }

    fn transformed_and_declared(transform: PaintTransform, child: impl IntoElement) -> Transformed {
        Transformed {
            transform,
            declare: true,
            child: child.into_any_element(),
        }
    }

    impl IntoElement for Transformed {
        type Element = Self;

        fn into_element(self) -> Self::Element {
            self
        }
    }

    impl Element for Transformed {
        type RequestLayoutState = ();
        type PrepaintState = ();

        fn id(&self) -> Option<ElementId> {
            None
        }

        fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
            None
        }

        fn request_layout(
            &mut self,
            _: Option<&GlobalElementId>,
            _: Option<&InspectorElementId>,
            window: &mut Window,
            cx: &mut App,
        ) -> (LayoutId, ()) {
            (self.child.request_layout(window, cx), ())
        }

        fn prepaint(
            &mut self,
            _: Option<&GlobalElementId>,
            _: Option<&InspectorElementId>,
            _: Bounds<Pixels>,
            _: &mut (),
            window: &mut Window,
            cx: &mut App,
        ) {
            if self.declare {
                window.with_expected_paint_transform(self.transform, |window| {
                    self.child.prepaint(window, cx)
                });
            } else {
                self.child.prepaint(window, cx);
            }
        }

        fn paint(
            &mut self,
            _: Option<&GlobalElementId>,
            _: Option<&InspectorElementId>,
            _: Bounds<Pixels>,
            _: &mut (),
            _: &mut (),
            window: &mut Window,
            cx: &mut App,
        ) {
            window.with_paint_transform(self.transform, |window| self.child.paint(window, cx));
        }
    }

    fn painter(paint: impl 'static + FnOnce(&mut Window, &mut App)) -> impl IntoElement {
        canvas(|_, _, _| (), move |_, _, window, cx| paint(window, cx))
    }

    fn draw_scene<E: IntoElement, R>(
        cx: &mut VisualTestContext,
        build: impl FnOnce(&mut Window, &mut App) -> E,
        inspect: impl FnOnce(&Scene) -> R,
    ) -> R {
        cx.update(|window, cx| {
            window.next_frame.scene.clear();

            window.invalidator.set_phase(DrawPhase::Prepaint);
            let mut element = Drawable::new(build(window, cx).into_element());
            element.layout_as_root(size(px(400.), px(400.)).into(), window, cx);
            let view_id = cx.new(|_| ()).entity_id();
            window.with_rendered_view(view_id, |window| {
                window.with_absolute_element_offset(point(px(0.), px(0.)), |window| {
                    element.prepaint(window, cx)
                });
            });
            window.prepaint_deferred_draws(cx);

            window.invalidator.set_phase(DrawPhase::Paint);
            window.with_rendered_view(view_id, |window| element.paint(window, cx));
            window.paint_deferred_draws(cx);
            window.invalidator.set_phase(DrawPhase::None);

            let result = inspect(&window.next_frame.scene);
            window.next_frame.scene.clear();
            result
        })
    }

    #[track_caller]
    fn assert_close(actual: Bounds<ScaledPixels>, expected: Bounds<ScaledPixels>) {
        let tolerance = 0.001;
        let close = (actual.origin.x.0 - expected.origin.x.0).abs() < tolerance
            && (actual.origin.y.0 - expected.origin.y.0).abs() < tolerance
            && (actual.size.width.0 - expected.size.width.0).abs() < tolerance
            && (actual.size.height.0 - expected.size.height.0).abs() < tolerance;
        assert!(close, "expected {expected:?}, got {actual:?}");
    }

    fn device(x: f32, y: f32, width: f32, height: f32) -> Bounds<ScaledPixels> {
        Bounds {
            origin: point(ScaledPixels(x), ScaledPixels(y)),
            size: size(ScaledPixels(width), ScaledPixels(height)),
        }
    }

    fn logical(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(width), px(height)),
        }
    }

    #[test]
    fn maps_composes_and_inverts_value_geometry() {
        let identity = PaintTransform::identity();
        let outer = PaintTransform {
            scale: 0.5,
            translation: point(px(10.), px(-4.)),
        };
        let inner = PaintTransform {
            scale: 2.0,
            translation: point(px(3.), px(7.)),
        };
        let bounds = logical(3., 5., 7., 11.);
        let point = point(px(13.), px(-8.));

        assert!(identity.is_identity());
        assert_eq!(identity.map_bounds(bounds), bounds);
        assert_eq!(identity.compose(outer), outer);
        assert_eq!(outer.compose(identity), outer);
        assert_eq!(
            outer.compose(inner).map_bounds(bounds),
            outer.map_bounds(inner.map_bounds(bounds))
        );
        assert_eq!(outer.inverse_map_point(outer.map_point(point)), point);

        let roundtrip = outer.inverse_map_bounds(outer.map_bounds(bounds));
        assert!((roundtrip.origin.x.0 - bounds.origin.x.0).abs() < 0.001);
        assert!((roundtrip.origin.y.0 - bounds.origin.y.0).abs() < 0.001);
        assert!((roundtrip.size.width.0 - bounds.size.width.0).abs() < 0.001);
        assert!((roundtrip.size.height.0 - bounds.size.height.0).abs() < 0.001);
    }

    #[test]
    fn scale_about_preserves_its_origin() {
        let origin = point(px(30.), px(40.));
        let transform = PaintTransform::scale_about(0.75, origin);
        assert_eq!(transform.map_point(origin), origin);
    }

    #[test]
    fn fractional_device_geometry_scales_origin_and_extent() {
        let transform = PaintTransform {
            scale: 0.875,
            translation: point(px(3.25), px(-1.5)),
        };
        let bounds = Bounds {
            origin: point(ScaledPixels(10.5), ScaledPixels(20.25)),
            size: size(ScaledPixels(30.75), ScaledPixels(40.5)),
        };

        for scale_factor in [1.0, 2.0] {
            let mapped = transform.map_device_bounds(bounds, scale_factor);
            assert_eq!(mapped.size.width.0, 30.75 * 0.875);
            assert_eq!(mapped.size.height.0, 40.5 * 0.875);
            assert_eq!(mapped.origin.x.0, 10.5 * 0.875 + 3.25 * scale_factor);
            assert_eq!(mapped.origin.y.0, 20.25 * 0.875 - 1.5 * scale_factor);
        }
    }

    #[test]
    #[should_panic(expected = "finite and positive")]
    fn rejects_zero_scale() {
        PaintTransform::scale(0.0);
    }

    #[test]
    #[should_panic(expected = "finite and positive")]
    fn rejects_non_finite_scale() {
        PaintTransform::scale(f32::NAN);
    }

    #[gpui::test]
    fn nested_scopes_restore_transform_and_clip_after_unwind(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let restored = Rc::new(Cell::new(None));
        let recorded = restored.clone();

        let quads = draw_scene(
            cx,
            move |_, _| {
                div()
                    .absolute()
                    .size(px(30.))
                    .overflow_hidden()
                    .child(painter(move |window, _| {
                        let panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
                            window.with_paint_transform(PaintTransform::scale(0.5), |window| {
                                window.with_paint_transform(
                                    PaintTransform::translate(point(px(10.), px(0.))),
                                    |_| panic!("intentional paint unwind"),
                                )
                            });
                        }));
                        assert!(panic.is_err());
                        recorded.set(Some((window.paint_transform(), window.content_mask())));

                        window.with_paint_transform(PaintTransform::scale(0.5), |window| {
                            window.with_paint_transform(
                                PaintTransform::translate(point(px(10.), px(0.))),
                                |window| {
                                    window.paint_quad(fill(
                                        logical(0., 0., 40., 40.),
                                        hsla(0.5, 0.5, 0.5, 1.),
                                    ));
                                },
                            );
                        });
                        window
                            .paint_quad(fill(logical(0., 0., 40., 40.), hsla(0.25, 0.5, 0.5, 1.)));
                    }))
            },
            |scene| scene.quads.clone(),
        );

        let (transform, mask) = restored.get().unwrap();
        assert_eq!(transform, PaintTransform::identity());
        assert_eq!(mask.bounds, logical(0., 0., 30., 30.));
        assert_close(quads[0].bounds, device(10., 0., 40., 40.));
        assert_close(quads[1].bounds, device(0., 0., 80., 80.));
        assert_close(quads[0].content_mask.bounds, device(0., 0., 60., 60.));
    }

    #[gpui::test]
    fn transforms_primitives_layers_and_device_space_parameters(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let transform = PaintTransform {
            scale: 0.5,
            translation: point(px(10.), px(20.)),
        };

        let (quad, shadow, underline, path, layer, backdrop, filter) = draw_scene(
            cx,
            move |_, _| {
                transformed(
                    transform,
                    painter(|window, _| {
                        let bounds = logical(40., 80., 100., 60.);
                        window.paint_drop_shadows(
                            bounds,
                            Corners::all(px(12.)),
                            &[BoxShadow {
                                color: hsla(0., 0., 0., 0.5),
                                offset: point(px(4.), px(8.)),
                                blur_radius: px(20.),
                                spread_radius: px(6.),
                                inset: false,
                            }],
                        );
                        window.paint_quad(crate::quad(
                            bounds,
                            Corners::all(px(12.)),
                            hsla(0.5, 0.5, 0.5, 1.),
                            Edges::all(px(4.)),
                            hsla(0., 0., 0., 1.),
                            crate::BorderStyle::Solid,
                        ));
                        window.paint_underline(
                            point(px(40.), px(145.)),
                            px(100.),
                            &UnderlineStyle {
                                thickness: px(4.),
                                color: Some(hsla(0., 0., 0., 1.)),
                                wavy: false,
                            },
                        );
                        let mut path = crate::Path::new(point(px(40.), px(80.)));
                        path.line_to(point(px(80.), px(80.)));
                        path.line_to(point(px(80.), px(120.)));
                        path.line_to(point(px(40.), px(80.)));
                        window.paint_path(path, hsla(0., 1., 0.5, 1.));
                        window.paint_layer(bounds, |window| {
                            window.paint_quad(fill(bounds, hsla(0.25, 0.5, 0.5, 1.)));
                        });
                        window.paint_backdrop_filter(
                            bounds,
                            Corners::all(px(12.)),
                            &[Filter::Blur(px(8.))],
                        );
                        window.with_filter_layer(
                            bounds,
                            Corners::all(px(12.)),
                            &[Filter::Blur(px(8.))],
                            |_| {},
                        );
                    }),
                )
            },
            |scene| {
                let layer = scene
                    .paint_operations
                    .iter()
                    .find_map(|operation| match operation {
                        crate::PaintOperation::StartLayer(bounds) => Some(*bounds),
                        _ => None,
                    })
                    .unwrap();
                (
                    scene.quads[0],
                    scene.shadows[0],
                    scene.underlines[0],
                    scene.paths[0].clone(),
                    layer,
                    scene.backdrop_filters[0].clone(),
                    scene.filter_boundaries[0].clone(),
                )
            },
        );

        assert_close(quad.bounds, device(60., 120., 100., 60.));
        assert_eq!(quad.corner_radii.top_left.0, 12.);
        assert_eq!(quad.border_widths.left.0, 4.);
        assert_close(shadow.bounds, device(58., 122., 112., 72.));
        assert_eq!(shadow.blur_radius.0, 20.);
        assert_close(underline.bounds, device(60., 185., 100., 4.));
        assert_close(path.bounds, device(60., 120., 40., 40.));
        assert_close(layer, device(60., 120., 100., 60.));
        assert_close(backdrop.bounds, device(60., 120., 100., 60.));
        assert_eq!(backdrop.filters[0], Filter::Blur(px(8.)).scale(1.));
        assert_close(filter.bounds, device(60., 120., 100., 60.));
        assert_eq!(filter.filters[0], Filter::Blur(px(8.)).scale(1.));
    }

    #[gpui::test]
    fn transformed_descendants_cannot_expand_ancestor_clips(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let quad = draw_scene(
            cx,
            |_, _| {
                div()
                    .absolute()
                    .w(px(30.))
                    .h(px(200.))
                    .overflow_hidden()
                    .child(transformed(
                        PaintTransform::scale(0.5),
                        div()
                            .absolute()
                            .w(px(200.))
                            .h(px(40.))
                            .overflow_hidden()
                            .child(painter(|window, _| {
                                window.paint_quad(fill(
                                    logical(0., 0., 400., 400.),
                                    hsla(0.5, 0.5, 0.5, 1.),
                                ));
                            })),
                    ))
            },
            |scene| scene.quads[0],
        );

        assert_close(quad.content_mask.bounds, device(0., 0., 60., 40.));
    }

    #[gpui::test]
    fn hitboxes_remain_in_layout_space(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let recorded = Rc::new(Cell::new(None));
        let sink = recorded.clone();
        draw_scene(
            cx,
            move |_, _| {
                div()
                    .absolute()
                    .size(px(50.))
                    .overflow_hidden()
                    .child(transformed_and_declared(
                        PaintTransform::scale(0.5),
                        canvas(
                            move |_, window, _| {
                                let hitbox = window.insert_hitbox(
                                    logical(0., 0., 200., 200.),
                                    HitboxBehavior::Normal,
                                );
                                sink.set(Some((hitbox.bounds, hitbox.content_mask.bounds)));
                            },
                            |_, _, _, _| {},
                        ),
                    ))
            },
            |_| (),
        );

        let (bounds, mask) = recorded.get().unwrap();
        assert_eq!(bounds, logical(0., 0., 200., 200.));
        assert_eq!(mask, logical(0., 0., 50., 50.));
    }

    #[gpui::test]
    fn deferred_draws_capture_their_declared_transform(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let quad = draw_scene(
            cx,
            |_, _| {
                transformed_and_declared(
                    PaintTransform {
                        scale: 0.5,
                        translation: point(px(10.), px(20.)),
                    },
                    deferred(anchored().position(point(px(40.), px(80.))).child(painter(
                        |window, _| {
                            window.paint_quad(fill(
                                logical(40., 80., 100., 60.),
                                hsla(0.5, 0.5, 0.5, 1.),
                            ));
                        },
                    ))),
                )
            },
            |scene| scene.quads[0],
        );

        assert_close(quad.bounds, device(60., 120., 100., 60.));
    }

    struct CachedLeaf {
        renders: Rc<Cell<usize>>,
        deferred: bool,
    }

    impl Render for CachedLeaf {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let child = painter(|window, _| {
                window.paint_quad(fill(logical(0., 0., 40., 20.), hsla(0.5, 0.5, 0.5, 1.)));
            });
            if self.deferred {
                deferred(child).into_any_element()
            } else {
                child.into_any_element()
            }
        }
    }

    struct CachingRoot {
        leaf: crate::Entity<CachedLeaf>,
        scale: f32,
        declare: bool,
    }

    impl Render for CachingRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let child = AnyView::from(self.leaf.clone())
                .cached(StyleRefinement::default().w(px(40.)).h(px(20.)));
            if self.declare {
                transformed_and_declared(PaintTransform::scale(self.scale), child)
                    .into_any_element()
            } else {
                transformed(PaintTransform::scale(self.scale), child).into_any_element()
            }
        }
    }

    fn widest_quad(cx: &mut VisualTestContext) -> f32 {
        cx.update(|window, _| {
            window
                .rendered_frame
                .scene
                .quads
                .iter()
                .map(|quad| quad.bounds.size.width.0)
                .fold(0.0, f32::max)
        })
    }

    fn assert_cached_transform_behavior(cx: &mut TestAppContext, declare: bool, deferred: bool) {
        let renders = Rc::new(Cell::new(0));
        let leaf = cx.new(|_| CachedLeaf {
            renders: renders.clone(),
            deferred,
        });
        let (root, visual) = cx.add_window_view(|_, _| CachingRoot {
            leaf,
            scale: 2.0,
            declare,
        });
        visual.run_until_parked();

        assert_eq!(widest_quad(visual), 160.0);
        let first_frame = renders.get();
        root.update(visual, |_, cx| cx.notify());
        visual.run_until_parked();
        if declare {
            assert_eq!(renders.get(), first_frame);
        } else {
            assert!(renders.get() > first_frame);
        }
        assert_eq!(widest_quad(visual), 160.0);

        let before_change = renders.get();
        root.update(visual, |root, cx| {
            root.scale = 3.0;
            cx.notify();
        });
        visual.run_until_parked();
        assert!(renders.get() > before_change);
        assert_eq!(widest_quad(visual), 240.0);
    }

    #[gpui::test]
    fn cached_views_reuse_only_at_the_recorded_transform(cx: &mut TestAppContext) {
        for declare in [false, true] {
            assert_cached_transform_behavior(cx, declare, false);
        }
    }

    #[gpui::test]
    fn deferred_paint_ranges_reuse_only_at_their_captured_transform(cx: &mut TestAppContext) {
        assert_cached_transform_behavior(cx, true, true);
    }

    struct SquareGlyphTextSystem;

    impl PlatformTextSystem for SquareGlyphTextSystem {
        fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
            let side = (params.font_size.0 * params.scale_factor) as i32;
            Ok(Bounds {
                origin: point(DevicePixels(0), DevicePixels(-side)),
                size: size(DevicePixels(side), DevicePixels(side)),
            })
        }

        fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
            NoopTextSystem.add_fonts(fonts)
        }

        fn all_font_names(&self) -> Vec<String> {
            NoopTextSystem.all_font_names()
        }

        fn font_id(&self, descriptor: &Font) -> Result<FontId> {
            NoopTextSystem.font_id(descriptor)
        }

        fn font_metrics(&self, font_id: FontId) -> crate::FontMetrics {
            NoopTextSystem.font_metrics(font_id)
        }

        fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
            NoopTextSystem.typographic_bounds(font_id, glyph_id)
        }

        fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
            NoopTextSystem.advance(font_id, glyph_id)
        }

        fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
            NoopTextSystem.glyph_for_char(font_id, ch)
        }

        fn rasterize_glyph(
            &self,
            params: &RenderGlyphParams,
            raster_bounds: Bounds<DevicePixels>,
        ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
            NoopTextSystem.rasterize_glyph(params, raster_bounds)
        }

        fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
            NoopTextSystem.layout_line(text, font_size, runs)
        }

        fn recommended_rendering_mode(
            &self,
            font_id: FontId,
            font_size: Pixels,
        ) -> TextRenderingMode {
            NoopTextSystem.recommended_rendering_mode(font_id, font_size)
        }
    }

    #[test]
    fn glyph_emoji_image_and_svg_destinations_transform_without_rerasterizing() {
        let mut app = TestAppContext::build_with_text_system(
            TestDispatcher::new(0),
            Some("paint_transform"),
            Arc::new(SquareGlyphTextSystem),
        );
        let cx = app.add_empty_window();
        let (glyph, emoji, image, svg) = draw_scene(
            cx,
            |_, _| {
                transformed(
                    PaintTransform {
                        scale: 0.5,
                        translation: point(px(10.), px(20.)),
                    },
                    painter(|window, cx| {
                        window
                            .paint_glyph(
                                point(px(20.), px(40.)),
                                FontId(1),
                                GlyphId(1),
                                px(16.),
                                hsla(0., 0., 0., 1.),
                            )
                            .unwrap();
                        window
                            .paint_emoji(point(px(40.), px(40.)), FontId(1), GlyphId(2), px(16.))
                            .unwrap();

                        let bounds = logical(40., 80., 100., 60.);
                        let image = Arc::new(crate::RenderImage::new(vec![image::Frame::new(
                            image::ImageBuffer::new(8, 8),
                        )]));
                        window
                            .paint_image(bounds, bounds, Corners::all(px(12.)), image, 0, false)
                            .unwrap();
                        window
                            .paint_svg(
                                logical(20., 40., 20., 20.),
                                "paint-transform-test".into(),
                                Some(
                                    br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="black"/></svg>"#,
                                ),
                                crate::TransformationMatrix::unit(),
                                hsla(0., 0., 0., 1.),
                                cx,
                            )
                            .unwrap();
                    }),
                )
            },
            |scene| {
                (
                    scene.monochrome_sprites[0],
                    scene.polychrome_sprites[0],
                    scene.polychrome_sprites[1],
                    scene.monochrome_sprites[1],
                )
            },
        );

        assert_eq!(glyph.bounds.size.width.0, 16.);
        assert_eq!(emoji.bounds.size.width.0, 16.);
        assert_close(image.bounds, device(60., 120., 100., 60.));
        assert_close(svg.bounds, device(40., 80., 20., 20.));
        assert_eq!(image.corner_radii.top_left.0, 12.);
        assert_eq!(glyph.content_mask.bounds, emoji.content_mask.bounds);
        assert_eq!(image.content_mask.bounds, svg.content_mask.bounds);
    }
}
