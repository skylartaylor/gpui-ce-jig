use crate::{Bounds, IsZero, Pixels, Point, ScaledPixels, TransformationMatrix, Window, point};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

/// A uniform scale and translation applied to geometry painted inside
/// [`Window::with_paint_transform`].
///
/// The transform is baked into primitive geometry on the CPU. Rotation and
/// non-uniform scale are deliberately not representable because GPUI's
/// axis-aligned primitives cannot describe them exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintTransform {
    /// The uniform scale factor. It must be finite and non-negative.
    pub scale: f32,
    /// Translation in logical pixels, applied after the scale.
    pub translation: Point<Pixels>,
}

impl Default for PaintTransform {
    fn default() -> Self {
        Self::identity()
    }
}

impl PaintTransform {
    /// The transform that leaves geometry unchanged.
    pub fn identity() -> Self {
        Self {
            scale: 1.0,
            translation: Point::default(),
        }
    }

    /// A uniform scale about the window origin.
    pub fn scale(scale: f32) -> Self {
        Self {
            scale,
            translation: Point::default(),
        }
    }

    /// A translation in logical pixels.
    pub fn translate(translation: Point<Pixels>) -> Self {
        Self {
            scale: 1.0,
            translation,
        }
    }

    /// A uniform scale around a fixed point.
    pub fn scale_about(scale: f32, origin: Point<Pixels>) -> Self {
        Self {
            scale,
            translation: origin - origin * scale,
        }
    }

    /// Whether this transform leaves geometry unchanged.
    pub fn is_identity(&self) -> bool {
        self.scale == 1.0 && self.translation.is_zero()
    }

    /// The transform that applies `inner` first and then `self`.
    pub fn compose(self, inner: Self) -> Self {
        Self {
            scale: self.scale * inner.scale,
            translation: inner.translation * self.scale + self.translation,
        }
    }

    /// Map a point from this transform's coordinate space into its parent.
    pub fn map_point(&self, point: Point<Pixels>) -> Point<Pixels> {
        point * self.scale + self.translation
    }

    /// Map a point from the parent coordinate space into this transform.
    ///
    /// A zero scale has no inverse and returns the origin.
    pub fn invert_point(&self, point: Point<Pixels>) -> Point<Pixels> {
        if self.scale == 0.0 {
            return Point::default();
        }
        (point - self.translation) * self.scale.recip()
    }

    /// Map bounds from this transform's coordinate space into its parent.
    pub fn map_bounds(&self, mut bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        bounds *= self.scale;
        bounds + self.translation
    }

    /// Map bounds from the parent coordinate space into this transform.
    ///
    /// A zero scale has no inverse and returns empty bounds.
    pub fn invert_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        if self.scale == 0.0 {
            return Bounds::default();
        }
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
    pub(crate) fn to_device_matrix(&self, scale_factor: f32) -> TransformationMatrix {
        let translation = self.device_translation(scale_factor);
        TransformationMatrix {
            rotation_scale: [[self.scale, 0.0], [0.0, self.scale]],
            translation: [translation.x.0, translation.y.0],
        }
    }
}

#[inline]
#[track_caller]
fn debug_assert_transform(transform: &PaintTransform) {
    debug_assert!(
        transform.scale.is_finite() && transform.scale >= 0.0,
        "a paint transform's scale must be finite and non-negative, got {}",
        transform.scale
    );
}

impl Window {
    /// Compose a uniform paint transform around `f`.
    ///
    /// This affects painted geometry only. Layout, hit testing, and
    /// accessibility bounds remain at their final untransformed positions.
    pub fn with_paint_transform<R>(
        &mut self,
        transform: PaintTransform,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.invalidator.debug_assert_paint();
        debug_assert_transform(&transform);

        if transform.is_identity() {
            return f(self);
        }

        let previous = self.paint_transform;
        self.paint_transform = previous.compose(transform);
        let mask = crate::ContentMask {
            bounds: transform.invert_bounds(self.content_mask().bounds),
        };
        self.content_mask_stack.push(mask);

        let result = catch_unwind(AssertUnwindSafe(|| f(self)));

        self.content_mask_stack.pop();
        self.paint_transform = previous;
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    }

    /// Declare during prepaint which transform paint will apply to the same
    /// subtree, allowing cached views to be reused safely.
    pub fn with_expected_paint_transform<R>(
        &mut self,
        transform: PaintTransform,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.invalidator.debug_assert_prepaint();
        debug_assert_transform(&transform);

        if transform.is_identity() {
            return f(self);
        }

        let previous = self.paint_transform;
        self.paint_transform = previous.compose(transform);
        let result = catch_unwind(AssertUnwindSafe(|| f(self)));
        self.paint_transform = previous;
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    }

    /// The transform currently applied during paint or expected during
    /// prepaint.
    pub fn paint_transform(&self) -> PaintTransform {
        self.paint_transform
    }

    #[inline]
    pub(crate) fn paint_transform_folded(&self) -> (f32, Point<ScaledPixels>) {
        let scale_factor = self.scale_factor();
        let transform = self.paint_transform;
        (
            scale_factor * transform.scale,
            transform.device_translation(scale_factor),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        self as gpui, AnyElement, AnyView, App, AppContext as _, Bounds, BoxShadow, Context,
        Corners, DevicePixels, DrawPhase, Drawable, Edges, Element, ElementId, Font, FontId,
        FontRun, GlobalElementId, GlyphId, HitboxBehavior, InspectorElementId, IntoElement,
        LayoutId, LineLayout, NoopTextSystem, ParentElement, PlatformTextSystem, Render,
        RenderGlyphParams, Size, StyleRefinement, Styled, TestAppContext, TestDispatcher,
        TextRenderingMode, UnderlineStyle, VisualTestContext, canvas, div, fill, hsla, px,
        scene::Scene, size,
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
            window.with_absolute_element_offset(point(px(0.), px(0.)), |window| {
                element.prepaint(window, cx)
            });

            window.invalidator.set_phase(DrawPhase::Paint);
            element.paint(window, cx);
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
    fn identity_composition_and_inverse_are_stable() {
        let identity = PaintTransform::identity();
        let transform = PaintTransform {
            scale: 0.5,
            translation: point(px(10.), px(-4.)),
        };
        let inner = PaintTransform {
            scale: 2.0,
            translation: point(px(3.), px(7.)),
        };
        let bounds = logical(3., 5., 7., 11.);

        assert!(identity.is_identity());
        assert_eq!(identity.map_bounds(bounds), bounds);
        assert_eq!(identity.compose(transform), transform);
        assert_eq!(transform.compose(identity), transform);
        assert_eq!(
            transform.compose(inner).map_bounds(bounds),
            transform.map_bounds(inner.map_bounds(bounds))
        );

        let roundtrip = transform.invert_bounds(transform.map_bounds(bounds));
        assert!((roundtrip.origin.x.0 - bounds.origin.x.0).abs() < 0.001);
        assert!((roundtrip.origin.y.0 - bounds.origin.y.0).abs() < 0.001);
        assert!((roundtrip.size.width.0 - bounds.size.width.0).abs() < 0.001);
        assert!((roundtrip.size.height.0 - bounds.size.height.0).abs() < 0.001);
        assert_eq!(
            PaintTransform::scale(0.0).invert_bounds(bounds),
            Bounds::default()
        );
    }

    #[test]
    fn fractional_transform_maps_consistently_at_one_and_two_x() {
        let transform = PaintTransform {
            scale: 0.875,
            translation: point(px(3.25), px(-1.5)),
        };
        let logical = device(10.5, 20.25, 30.75, 40.5);

        for scale_factor in [1.0, 2.0] {
            let mapped = transform.map_device_bounds(logical, scale_factor);
            let matrix = transform.to_device_matrix(scale_factor);
            let expected_origin = point(
                ScaledPixels(logical.origin.x.0 * transform.scale + matrix.translation[0]),
                ScaledPixels(logical.origin.y.0 * transform.scale + matrix.translation[1]),
            );
            assert_eq!(mapped.origin, expected_origin);
            assert_eq!(
                mapped.size,
                size(
                    ScaledPixels(logical.size.width.0 * transform.scale),
                    ScaledPixels(logical.size.height.0 * transform.scale),
                )
            );
        }
    }

    #[gpui::test]
    fn nested_transforms_compose_and_restore_after_panics(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            window.invalidator.set_phase(DrawPhase::Prepaint);
            let panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
                window.with_expected_paint_transform(PaintTransform::scale(0.5), |_| {
                    panic!("intentional prepaint unwind")
                });
            }));
            assert!(panic.is_err());
            assert_eq!(window.paint_transform(), PaintTransform::identity());
            window.invalidator.set_phase(DrawPhase::None);
        });
        let transforms_after_panic = Rc::new(Cell::new(None));
        let recorded = transforms_after_panic.clone();

        let quads = draw_scene(
            cx,
            move |_, _| {
                painter(move |window, _| {
                    let panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        window.with_paint_transform(PaintTransform::scale(0.5), |window| {
                            window.with_paint_transform(
                                PaintTransform::translate(point(px(100.), px(0.))),
                                |_| panic!("intentional paint unwind"),
                            )
                        });
                    }));
                    assert!(panic.is_err());
                    recorded.set(Some(window.paint_transform()));

                    window.with_paint_transform(PaintTransform::scale(0.5), |window| {
                        window.with_paint_transform(
                            PaintTransform::translate(point(px(100.), px(0.))),
                            |window| {
                                window.paint_quad(fill(
                                    logical(0., 0., 40., 40.),
                                    hsla(0.5, 0.5, 0.5, 1.),
                                ));
                            },
                        );
                    });
                    window.paint_quad(fill(logical(0., 0., 40., 40.), hsla(0.25, 0.5, 0.5, 1.)));
                })
            },
            |scene| scene.quads.clone(),
        );

        assert_eq!(
            transforms_after_panic.get(),
            Some(PaintTransform::identity())
        );
        assert_close(quads[0].bounds, device(100., 0., 40., 40.));
        assert_close(quads[1].bounds, device(0., 0., 80., 80.));
    }

    #[gpui::test]
    fn primitive_geometry_and_layers_are_transformed(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let transform = PaintTransform {
            scale: 0.5,
            translation: point(px(10.), px(20.)),
        };

        let (quad, shadow, underline, path, layer) = draw_scene(
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
                    .expect("expected layer start");
                (
                    scene.quads[0],
                    scene.shadows[0],
                    scene.underlines[0],
                    scene.paths[0].clone(),
                    layer,
                )
            },
        );

        assert_close(quad.bounds, device(60., 120., 100., 60.));
        assert!((quad.corner_radii.top_left.0 - 12.).abs() < 0.001);
        assert!((quad.border_widths.left.0 - 4.).abs() < 0.001);
        assert_close(shadow.bounds, device(58., 122., 112., 72.));
        assert!((shadow.blur_radius.0 - 20.).abs() < 0.001);
        assert_close(underline.bounds, device(60., 185., 100., 4.));
        assert_close(path.bounds, device(60., 120., 40., 40.));
        assert_close(layer, device(60., 120., 100., 60.));
    }

    #[gpui::test]
    fn clips_intersect_across_transform_boundaries(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();

        let quad = draw_scene(
            cx,
            |_, _| {
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .w(px(30.))
                    .h(px(200.))
                    .overflow_hidden()
                    .child(transformed(
                        PaintTransform::scale(0.5),
                        div()
                            .absolute()
                            .left(px(0.))
                            .top(px(0.))
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
        for declared in [false, true] {
            let recorded = Rc::new(Cell::new(None));
            let sink = recorded.clone();
            draw_scene(
                cx,
                move |_, _| {
                    div()
                        .absolute()
                        .left(px(0.))
                        .top(px(0.))
                        .w(px(50.))
                        .h(px(50.))
                        .overflow_hidden()
                        .child({
                            let child = canvas(
                                move |_, window, _| {
                                    let hitbox = window.insert_hitbox(
                                        logical(0., 0., 200., 200.),
                                        HitboxBehavior::Normal,
                                    );
                                    sink.set(Some((hitbox.bounds, hitbox.content_mask.bounds)));
                                },
                                |_, _, _, _| {},
                            );
                            if declared {
                                transformed_and_declared(PaintTransform::scale(0.5), child)
                                    .into_any_element()
                            } else {
                                transformed(PaintTransform::scale(0.5), child).into_any_element()
                            }
                        })
                },
                |_| (),
            );

            let (bounds, mask) = recorded.get().expect("hitbox was not inserted");
            assert_eq!(bounds, logical(0., 0., 200., 200.));
            assert_eq!(mask, logical(0., 0., 50., 50.));
        }
    }

    struct CachedLeaf {
        renders: Rc<Cell<usize>>,
    }

    impl Render for CachedLeaf {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            painter(|window, _| {
                window.paint_quad(fill(logical(0., 0., 40., 20.), hsla(0.5, 0.5, 0.5, 1.)));
            })
        }
    }

    struct CachingRoot {
        leaf: crate::Entity<CachedLeaf>,
        scale: f32,
        declare: bool,
    }

    impl Render for CachingRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let cached = AnyView::from(self.leaf.clone())
                .cached(StyleRefinement::default().w(px(40.)).h(px(20.)));
            let transform = PaintTransform::scale(self.scale);
            if self.declare {
                transformed_and_declared(transform, cached).into_any_element()
            } else {
                transformed(transform, cached).into_any_element()
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

    #[gpui::test]
    fn cached_views_reuse_only_under_the_transform_they_were_painted_with(cx: &mut TestAppContext) {
        for declare in [false, true] {
            let renders = Rc::new(Cell::new(0));
            let leaf = cx.new(|_| CachedLeaf {
                renders: renders.clone(),
            });
            let (root, visual) = cx.add_window_view(|_, _| CachingRoot {
                leaf,
                scale: 2.0,
                declare,
            });
            visual.run_until_parked();

            assert_eq!(widest_quad(visual), 160.0);
            let first_frame = renders.get();
            assert!(first_frame > 0);

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

    fn text_app() -> TestAppContext {
        TestAppContext::build_with_text_system(
            TestDispatcher::new(0),
            Some("paint_transform"),
            Arc::new(SquareGlyphTextSystem),
        )
    }

    #[test]
    fn glyph_and_emoji_sprites_are_transformed() {
        let mut app = text_app();
        let cx = app.add_empty_window();

        let (glyph, emoji) = draw_scene(
            cx,
            |_, _| {
                transformed(
                    PaintTransform {
                        scale: 0.5,
                        translation: point(px(8.), px(0.)),
                    },
                    painter(|window, _| {
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
                    }),
                )
            },
            |scene| (scene.monochrome_sprites[0], scene.polychrome_sprites[0]),
        );

        assert!((glyph.bounds.size.width.0 - 16.).abs() < 0.001);
        assert!((emoji.bounds.size.width.0 - 16.).abs() < 0.001);
        assert!(glyph.bounds.origin.x.0 >= 16.);
        assert!(emoji.bounds.origin.x.0 > glyph.bounds.origin.x.0);
        assert_eq!(glyph.content_mask.bounds, emoji.content_mask.bounds);
    }

    #[gpui::test]
    fn image_and_svg_sprites_preserve_rasters_and_transform_geometry(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let (image, svg) = draw_scene(
            cx,
            |_, _| {
                transformed(
                    PaintTransform {
                        scale: 0.5,
                        translation: point(px(10.), px(20.)),
                    },
                    painter(|window, cx| {
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
            |scene| (scene.polychrome_sprites[0], scene.monochrome_sprites[0]),
        );

        assert_close(image.bounds, device(60., 120., 100., 60.));
        assert!((image.corner_radii.top_left.0 - 12.).abs() < 0.001);
        assert!((svg.transformation.rotation_scale[0][0] - 0.5).abs() < 0.001);
        assert!((svg.transformation.rotation_scale[1][1] - 0.5).abs() < 0.001);
        assert!((svg.transformation.translation[0] - 20.).abs() < 0.001);
        assert!((svg.transformation.translation[1] - 40.).abs() < 0.001);
        assert_eq!(image.content_mask.bounds, svg.content_mask.bounds);
    }
}
