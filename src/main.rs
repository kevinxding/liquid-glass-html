use gpui::{prelude::*, *};
use gpui_glass::controls::{ControlsChanged, GlassControls};
use gpui_glass::{
    glass::{GlassParams, GlassRenderer, ReflectionParams},
    shape::Shape,
};
use std::sync::Arc;

// The upstream input engine provides native text services and Unicode cursor movement.
mod editor;
pub mod scroll_bounce;
use scroll_bounce::ScrollBounce;
actions!(
    glass_demo,
    [Backspace, Delete, End, Home, Left, Right, Quit]
);

const INK: u32 = 0x202c2a;
const MUTED: u32 = 0x7d8984;
const GREEN: u32 = 0x396958;
const PAPER: u32 = 0xf5f6f0;
struct Demo {
    glass: Arc<GlassRenderer>,
    canvas_scroll: ScrollHandle,
    params: GlassParams,
    shape: Shape,
    enabled: bool,
    controls: Entity<GlassControls>,
    dark: bool,
    editor: Entity<editor::Editor>,
    animate: bool,
    reflection_targets: bool,
    lighting_preview: u8,
    phase: f32,
}
impl Demo {
    fn glass_layer(&self, id: u64, shape: Shape, _scale: f32) -> AnyElement {
        if !self.enabled {
            if shape == Shape::Capsule {
                return gpui_smooth::shape_layer(
                    gpui_smooth::SmoothShape::capsule(self.params.smoothing),
                    gpui_smooth::ShapeStyle::new(rgba(if self.dark {
                        0x202825d0
                    } else {
                        0xffffffb0
                    })),
                )
                .absolute()
                .inset_0()
                .into_any_element();
            }
            return div()
                .absolute()
                .inset_0()
                .rounded(px(32.))
                .rounded_smoothing(self.params.smoothing)
                .bg(rgba(if self.dark { 0x202825d0 } else { 0xffffffb0 }))
                .into_any_element();
        }
        let mut params = self.params;
        params.tint_color = [if self.dark { 0.06 } else { 0.97 }; 3];
        if id == 4 && self.lighting_preview != 0 {
            params.surface = Some(
                [if self.lighting_preview == 2 {
                    0.22
                } else {
                    0.94
                }; 3],
            );
            params.reflection = ReflectionParams::disabled();
        }
        gpui_glass::glass_layer(self.glass.clone(), id, params, shape)
            .absolute()
            .inset_0()
            .into_any_element()
    }
    fn toolbar_button(&self, id: u64, label: &'static str, width: f32, scale: f32) -> AnyElement {
        div()
            .id(ElementId::Name(format!("toolbar-{id}").into()))
            .relative()
            .w(px(width))
            .h(px(50.))
            .cursor_pointer()
            .child(self.glass_layer(id, Shape::Capsule, scale))
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label),
            )
            .on_click(|_, _, _| {})
            .into_any_element()
    }
    fn theme_button(&self, scale: f32, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("toggle-theme")
            .relative()
            .w(px(78.))
            .h(px(50.))
            .cursor_pointer()
            .child(self.glass_layer(7, Shape::Capsule, scale))
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .child(if self.dark { "☀ Light" } else { "☾ Dark" }),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.dark = !this.dark;
                let dark = this.dark;
                this.controls.update(cx, |controls, cx| {
                    controls.dark = dark;
                    cx.notify();
                });
                cx.notify();
            }))
            .into_any_element()
    }
    fn inspector(&self, cx: &mut Context<Self>) -> AnyElement {
        let background = if self.enabled {
            gpui_glass::glass_layer(
                self.glass.clone(),
                6,
                self.params.reflection_only(if self.dark {
                    [0.12, 0.14, 0.13]
                } else {
                    [250. / 255., 251. / 255., 247. / 255.]
                }),
                Shape::Rectangle,
            )
            .absolute()
            .inset_0()
            .into_any_element()
        } else {
            div()
                .absolute()
                .inset_0()
                .bg(rgb(if self.dark { 0x1f2421 } else { 0xfafbf7 }))
                .into_any_element()
        };
        div()
            .id("inspector")
            .relative()
            .w(px(304.))
            .flex_shrink_0()
            .h_full()
            .child(background)
            .child(
                div()
                    .id("inspector-scroll")
                    .absolute()
                    .inset_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .p(px(26.))
                            .text_size(px(12.))
                            .line_height(px(17.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(eyebrow("MATERIAL LAB"))
                                    .child(div().size(px(7.)).rounded_full().bg(rgb(0x79a589))),
                            )
                            .child(
                                div()
                                    .mt(px(14.))
                                    .text_size(px(25.))
                                    .line_height(px(32.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Liquid glass"),
                            )
                            .child(
                                div()
                                    .mt(px(8.))
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .text_color(rgb(MUTED))
                                    .child(
                                        "A lens for the interface. Move the light, bend the view.",
                                    ),
                            )
                            .child(div().mt(px(22.)).child(self.controls.clone()))
                            .child(
                                div()
                                    .mt(px(13.))
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .text_size(px(12.))
                                    .child("Animate backdrop")
                                    .child(
                                        div()
                                            .id("toggle-motion")
                                            .w(px(34.))
                                            .h(px(20.))
                                            .p(px(3.))
                                            .rounded_full()
                                            .bg(rgb(if self.animate { GREEN } else { 0xc5cdc1 }))
                                            .cursor_pointer()
                                            .flex()
                                            .when(self.animate, |d| d.justify_end())
                                            .child(
                                                div()
                                                    .size(px(14.))
                                                    .rounded_full()
                                                    .bg(rgb(0xffffff)),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.animate = !this.animate;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(13.))
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .text_size(px(12.))
                                    .child("Reflection targets")
                                    .child(
                                        div()
                                            .id("toggle-reflection-targets")
                                            .w(px(34.))
                                            .h(px(20.))
                                            .p(px(3.))
                                            .rounded_full()
                                            .bg(rgb(if self.reflection_targets {
                                                GREEN
                                            } else {
                                                0xc5cdc1
                                            }))
                                            .cursor_pointer()
                                            .flex()
                                            .when(self.reflection_targets, |d| d.justify_end())
                                            .child(
                                                div()
                                                    .size(px(14.))
                                                    .rounded_full()
                                                    .bg(rgb(0xffffff)),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.reflection_targets = !this.reflection_targets;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(22.))
                                    .pt(px(15.))
                                    .border_t_1()
                                    .border_color(rgb(0xe4e9df))
                                    .text_size(px(10.))
                                    .line_height(px(16.))
                                    .text_color(rgb(MUTED))
                                    .child("WGPU / native backdrop refraction")
                                    .child(
                                        div().child("Scroll the canvas to look through the lens."),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}
fn eyebrow(label: &'static str) -> impl IntoElement {
    div()
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(label)
}
fn chip(dark: bool, label: &'static str) -> impl IntoElement {
    div()
        .px(px(13.))
        .py(px(7.))
        .rounded(px(14.))
        .rounded_smoothing(0.75)
        .bg(rgb(if dark { 0x303e34 } else { 0xe7ebdf }))
        .text_size(px(11.))
        .child(label)
}
fn photo(path: &'static str) -> impl IntoElement {
    static ALPINE: std::sync::OnceLock<Arc<Image>> = std::sync::OnceLock::new();
    static FOREST: std::sync::OnceLock<Arc<Image>> = std::sync::OnceLock::new();
    static BALLOONS: std::sync::OnceLock<Arc<Image>> = std::sync::OnceLock::new();
    static COAST: std::sync::OnceLock<Arc<Image>> = std::sync::OnceLock::new();
    let (slot, bytes): (_, &[u8]) = if path == "alpine.jpg" {
        (&ALPINE, include_bytes!("../assets/alpine.jpg"))
    } else if path == "balloons.jpg" {
        (&BALLOONS, include_bytes!("../assets/balloons.jpg"))
    } else if path == "coast.jpg" {
        (&COAST, include_bytes!("../assets/coast.jpg"))
    } else {
        (&FOREST, include_bytes!("../assets/forest.jpg"))
    };
    img(slot
        .get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Jpeg, bytes.to_vec())))
        .clone())
    .absolute()
    .inset_0()
    .w_full()
    .h_full()
    .object_fit(ObjectFit::Cover)
    .rounded(px(22.))
    .rounded_smoothing(0.8)
}
impl Render for Demo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scale = window.scale_factor();
        if self.animate {
            self.phase += 0.018;
            window.request_animation_frame();
        }
        let focus = self.editor.read(cx).focus_handle.clone();
        div().id("app").size_full().flex().bg(rgb(if self.dark {0x171d1a} else {PAPER})).text_color(rgb(if self.dark {0xe5ece6} else {INK})).font_family(".AppleSystemUIFont")
            .child(div().flex_1().min_w(px(540.)).h_full().relative().overflow_hidden()
                .child(ScrollBounce::new("canvas-bounce", &self.canvas_scroll,
                    div().id("canvas-scroll").absolute().inset_0().overflow_y_scroll().track_scroll(&self.canvas_scroll)
                    .child(div().px(px(44.)).pt(px(120.)).pb(px(140.)).flex().flex_col()
                        .child(div().flex().items_center().justify_between().child(eyebrow("FIELDNOTES  /  ISSUE Nº 004")).child(eyebrow("A STUDY IN SLOW LIVING")))
                        .child(div().mt(px(24.)).text_size(px(64.)).line_height(px(67.)).font_family("Times New Roman").child("A little further").child(div().italic().text_color(rgb(GREEN)).child("from ordinary.")))
                        .child(div().mt(px(20.)).max_w(px(460.)).text_size(px(14.)).line_height(px(23.)).text_color(rgb(if self.dark {0xabb9ae} else {0x728077})).child("Small discoveries. Wide-open spaces. A collection of places, objects, and ideas worth taking a moment for."))
                        .child(div().mt(px(22.)).flex().gap_2().child(chip(self.dark,"Wander slowly")).child(chip(self.dark,"Stay curious")).child(chip(self.dark,"Collect moments")))
                        .child(div().mt(px(28.)).w_full().h(px(330.)).flex_shrink_0().relative().overflow_hidden().child(photo("alpine.jpg"))
                            .child(div().absolute().left(px(23.)).bottom(px(23.)).text_color(rgb(0xffffff)).child(div().text_size(px(10.)).child("46° 35′ N  /  12° 14′ E")).child(div().mt(px(8.)).text_size(px(30.)).font_family("Times New Roman").child("Somewhere, slower."))))
                        .child(div().mt(px(18.)).flex().justify_between().text_size(px(11.)).text_color(rgb(MUTED)).child("01 — A quiet kind of grand").child("The Dolomites, Italy  ↗"))
                        .child(div().mt(px(32.)).flex().gap(px(22.)).child(div().flex_1().h(px(245.)).relative().overflow_hidden().child(photo("forest.jpg")))
                            .child(div().flex_1().pt(px(8.)).child(eyebrow("TAKE THE SCENIC ROUTE")).child(div().mt(px(15.)).text_size(px(33.)).font_family("Times New Roman").child("Nothing urgent.").child(div().italic().child("Everything alive.")))
                                .child(div().mt(px(16.)).text_size(px(13.)).line_height(px(22.)).text_color(rgb(MUTED)).child("Follow the shade. Listen to the leaves. Let the afternoon turn into something you hadn't planned."))
                                .child(div().id("read-note").mt(px(20.)).px(px(16.)).py(px(10.)).rounded(px(15.)).rounded_smoothing(0.8).bg(rgb(if self.dark {0x34443a} else {0xe0e7d8})).cursor_pointer().text_size(px(12.)).child("Read the note  ↗").on_click(|_,_,_|{}))))
                        .child(div().mt(px(44.)).child(eyebrow("A BRIGHTER DETOUR")))
                        .child(div().mt(px(14.)).w_full().h(px(390.)).flex_shrink_0().relative().overflow_hidden().child(photo("balloons.jpg"))
                            .child(div().absolute().left(px(24.)).bottom(px(24.)).text_color(rgb(0xffffff)).text_size(px(34.)).font_family("Times New Roman").child("A sky full of possibility.")))
                        .child(div().mt(px(16.)).text_size(px(14.)).line_height(px(23.)).text_color(rgb(MUTED)).child("Red, saffron, ultramarine. Let the morning choose a different direction."))
                        .child(div().mt(px(30.)).w_full().h(px(430.)).flex_shrink_0().relative().overflow_hidden().child(photo("coast.jpg")))
                        .child(div().mt(px(18.)).flex().justify_between().text_size(px(11.)).text_color(rgb(MUTED)).child("03 — Every shade of summer").child("A postcard from the coast ↗"))
                        .child(div().mt(px(45.)).text_size(px(63.)).line_height(px(68.)).font_family("Times New Roman").child("Less, but lovelier."))
                        .child(div().mt(px(20.)).flex().gap_2().child(chip(self.dark,"Keep exploring  ↗")).child(chip(self.dark,"Make a little space  +"))))).enabled(true))
                .when(self.animate,|d|d.child(div().absolute().left(px(80.+self.phase.sin()*70.)).bottom(px(12.)).w(px(310.)).h(px(90.)).rounded(px(30.)).rounded_smoothing(0.8).bg(rgb(0xed936d))))
                .when(self.reflection_targets, |d| d.children(
                    [0xf44c76, 0xfcac32, 0x83bd3e, 0x28b8b5, 0x526afa, 0xb849de]
                        .into_iter().enumerate().map(|(i, color)| {
                            let distance = if self.animate { 10. + 100. * (0.5 + 0.5 * (self.phase + i as f32 * 0.4).sin()) } else { 12. };
                            div().absolute().right(px(distance)).top(px(155. + i as f32 * 90.))
                                .w(px(52.)).h(px(62.)).rounded(px(18.)).rounded_smoothing(0.8).bg(rgb(color))
                        })))
                .child(div().absolute().top(px(25.)).left(px(30.)).right(px(30.)).flex().items_center().justify_between()
                    .child(self.toolbar_button(1,"◈   Fieldnotes",152.,scale))
                    .child(div().flex().gap(px(10.)).child(self.toolbar_button(2,"⌕    +    ···",124.,scale)).child(self.toolbar_button(3,"♡   Save",96.,scale)).child(self.theme_button(scale,cx))))
                .child(div().absolute().right(px(32.)).top(px(305.)).w(px(170.)).h(px(110.))
                    .child(self.glass_layer(4,self.shape,scale))
                    .child(div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap(px(4.)).text_size(px(11.)).text_color(rgb(if self.lighting_preview == 2 || (self.lighting_preview==0 && self.dark) {0xdadada} else {INK})).child("LOOK CLOSER").child(div().text_size(px(24.)).font_family("Times New Roman").italic().child("Take a breath."))))
                .child(div().absolute().bottom(px(25.)).left(px(44.)).right(px(44.)).h(px(66.))
                    .child(self.glass_layer(5,Shape::Rounded,scale))
                    .child(div().relative().size_full().flex().items_center().gap(px(15.)).px(px(20.)).child(div().text_size(px(24.)).child("+"))
                        .child(div().id("floating-input").key_context("TextInput").track_focus(&focus).cursor(CursorStyle::IBeam).map(editor::standard_actions(self.editor.clone())).flex_1().h(px(24.)).overflow_hidden().text_size(px(14.)).line_height(px(24.)).child(self.editor.clone()))
                        .child(div().id("send").size(px(34.)).rounded_full().bg(rgb(GREEN)).text_color(rgb(0xffffff)).flex().items_center().justify_center().cursor_pointer().child("↑").on_click(|_,_,_|{})))))
            .child(self.inspector(cx))
    }
}
fn main() {
    env_logger::init();
    gpui_platform::application().run(|cx: &mut App| {
        // Match GPUI Kit's macOS reduced-motion initialization without polling.
        #[cfg(target_os = "macos")]
        cx.set_reduce_motion(
            objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion(),
        );
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, Some("TextInput")),
            KeyBinding::new("delete", Delete, Some("TextInput")),
            KeyBinding::new("left", Left, Some("TextInput")),
            KeyBinding::new("right", Right, Some("TextInput")),
            KeyBinding::new("home", Home, Some("TextInput")),
            KeyBinding::new("end", End, Some("TextInput")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1240.), px(890.)), cx);
        cx.open_window(
            WindowOptions::new()
                .titlebar(Some(TitlebarOptions {
                    title: Some("Fieldnotes — Liquid Glass Lab".into()),
                    ..Default::default()
                }))
                .window_bounds(Some(WindowBounds::Windowed(bounds)))
                .window_min_size(Some(size(px(960.), px(720.)))),
            |window, cx| {
                let controls = cx.new(|_| GlassControls::new(GlassParams::default()));
                cx.new(|cx| {
                    cx.subscribe(
                        &controls,
                        |this: &mut Demo, _, event: &ControlsChanged, cx| {
                            this.params = event.params;
                            this.shape = event.shape;
                            this.enabled = event.enabled;
                            this.lighting_preview = event.lighting_preview;
                            cx.notify();
                        },
                    )
                    .detach();
                    Demo {
                        glass: Arc::new(GlassRenderer::default()),
                        canvas_scroll: ScrollHandle::new(),
                        params: GlassParams::default(),
                        shape: Shape::Rounded,
                        enabled: true,
                        controls: controls.clone(),
                        dark: false,
                        editor: cx.new(|cx| editor::Editor::new("", window, cx)),
                        animate: false,
                        reflection_targets: false,
                        lighting_preview: 0,
                        phase: 0.,
                    }
                })
            },
        )
        .expect("open demo window");
        cx.activate(true);
    });
}
