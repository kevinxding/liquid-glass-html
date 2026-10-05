//! Prism: intentionally excessive animated glass/reflection stress workload.
//! The trading board is synthetic test content, not a product or appearance reference.
use gpui::{prelude::*, *};
use gpui_glass::{
    GlassParams, GlassRenderer, Shape,
    controls::{ControlsChanged, GlassControls},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
#[path = "market/motion.rs"]
mod motion;
use motion::Spring;
actions!(prism, [Quit]);
const COLORS: [u32; 6] = [0x35e3b1, 0xa889ff, 0xff689d, 0xffc65c, 0x41cfff, 0xff855f];
#[derive(Clone, Copy, PartialEq, Eq)]
enum Workload {
    Realistic,
    Prism,
    Cranked,
}
impl Workload {
    fn label(self) -> &'static str {
        match self {
            Self::Realistic => "Realistic",
            Self::Prism => "Prism",
            Self::Cranked => "Cranked",
        }
    }
    fn params(self) -> GlassParams {
        let mut p = GlassParams {
            opacity: 0.02,
            ..Default::default()
        };
        if self != Self::Realistic {
            p.reflection.diffuse_intensity = 0.7;
            p.reflection.diffuse_radius = 110.;
            p.reflection.dark_bias = 0.4;
        }
        if self == Self::Cranked {
            p.reflection.edge_intensity = 10.;
            p.reflection.edge_width = 12.;
            p.reflection.diffuse_intensity = 10.;
            p.reflection.diffuse_radius = 512.;
            p.reflection.sampling_radius = 240.;
            p.reflection.shadow_intensity = 2.;
        }
        p
    }
}
#[derive(Clone, Copy)]
struct LaunchConfig {
    workload: Workload,
    target_hz: u32,
    live: bool,
}
impl LaunchConfig {
    fn from_args(args: impl Iterator<Item = String>) -> anyhow::Result<Option<Self>> {
        let mut config = Self {
            workload: Workload::Prism,
            target_hz: 60,
            live: true,
        };
        for arg in args {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!(
                        "Prism stress test\n  --workload=realistic|prism|cranked\n  --fps=60|120\n  --paused"
                    );
                    return Ok(None);
                }
                "--workload=realistic" => config.workload = Workload::Realistic,
                "--workload=prism" => config.workload = Workload::Prism,
                "--workload=cranked" => config.workload = Workload::Cranked,
                "--fps=60" => config.target_hz = 60,
                "--fps=120" => config.target_hz = 120,
                "--paused" => config.live = false,
                _ => anyhow::bail!("Unknown argument {arg}; see --help"),
            }
        }
        Ok(Some(config))
    }
}
struct Market {
    renderer: Arc<GlassRenderer>,
    controls: Entity<GlassControls>,
    params: GlassParams,
    enabled: bool,
    shape: Shape,
    preview: u8,
    dark: bool,
    live: bool,
    phase: f32,
    last: Instant,
    morphs: [Spring; 3],
    pop: Spring,
    pop_kind: usize,
    selected: usize,
    scroll: ScrollHandle,
    frame_ms: f32,
    workload: Workload,
    custom_params: bool,
    target_hz: u32,
    next_tick: Instant,
    tick_pending: bool,
}
fn text(label: impl Into<SharedString>, size: f32) -> Div {
    div().text_size(px(size)).child(label.into())
}
fn tag(label: impl Into<SharedString>, color: u32) -> Div {
    text(label, 10.)
        .px_2()
        .py_1()
        .rounded_full()
        .text_color(rgb(color))
        .bg(rgba((color << 8) | 28))
}
fn chart(phase: f32, color: u32, kind: usize) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let w = f32::from(b.size.width);
            let h = f32::from(b.size.height);
            let pos = |x: f32, y: f32| b.origin + point(px(x), px(y));
            for i in 1..5 {
                window.paint_quad(fill(
                    Bounds::new(pos(0., h * i as f32 / 5.), size(px(w), px(1.))),
                    rgba(0x8a9bb71a),
                ));
            }
            if kind == 1 {
                for i in 0..48 {
                    let t = i as f32 / 48.;
                    let x = t * w;
                    let wave =
                        (t * 15. + phase).sin() * 0.19 + (t * 31. - phase * 0.7).sin() * 0.12;
                    let y = h * (0.5 + wave);
                    let d = (t * 43. + phase * 1.7).sin() * h * 0.09;
                    let c = if d > 0. { COLORS[0] } else { COLORS[2] };
                    window.paint_quad(fill(
                        Bounds::new(
                            pos(x + w / 110., y - d.abs() - 8.),
                            size(px(1.), px(d.abs() * 2. + 16.)),
                        ),
                        rgb(c),
                    ));
                    window.paint_quad(fill(
                        Bounds::new(
                            pos(x, y.min(y + d)),
                            size(px((w / 65.).max(2.)), px(d.abs().max(3.))),
                        ),
                        rgb(c),
                    ));
                }
            } else {
                for (line, &line_color) in
                    COLORS
                        .iter()
                        .enumerate()
                        .take(if kind == 2 { 3 } else { 1 })
                {
                    let c = if kind == 2 { line_color } else { color };
                    let mut p = PathBuilder::stroke(px(2.2));
                    for i in 0..110 {
                        let t = i as f32 / 109.;
                        let y = h
                            * (0.52
                                + 0.2 * (t * 10. + phase + line as f32).sin()
                                + 0.09 * (t * 27. - phase * 1.4).sin());
                        if i == 0 {
                            p.move_to(pos(t * w, y));
                        } else {
                            p.line_to(pos(t * w, y));
                        }
                    }
                    if let Ok(p) = p.build() {
                        window.paint_path(p, rgb(c));
                    }
                }
            }
        },
    )
    .size_full()
}
impl Market {
    fn animating(&self, reduced: bool) -> bool {
        (self.live && !reduced) || self.pop.active() || self.morphs.iter().any(Spring::active)
    }
    // Keep the display's pacing, but don't invalidate/draw on skipped refreshes.
    // A paused, settled workload leaves no callback or timer running.
    fn request_tick(entity: WeakEntity<Self>, window: &Window) {
        window.on_next_frame(move |window, cx| {
            let _ = entity.update(cx, |this, cx| {
                if !this.animating(cx.reduce_motion()) {
                    this.tick_pending = false;
                    return;
                }
                let now = Instant::now();
                let interval = Duration::from_secs_f64(1. / this.target_hz as f64);
                // Carry the fractional refresh remainder across frames: a 60fps
                // target also averages 60 on e.g. a 144Hz display, rather than 48.
                // Small deadline tolerance absorbs display-clock jitter.
                if now + Duration::from_micros(500) >= this.next_tick {
                    this.next_tick = if now.saturating_duration_since(this.next_tick) > interval {
                        now + interval
                    } else {
                        this.next_tick + interval
                    };
                    this.tick_pending = false;
                    cx.notify();
                } else {
                    Self::request_tick(cx.weak_entity(), window);
                }
            });
        });
    }
    fn set_workload(&mut self, workload: Workload, cx: &mut Context<Self>) {
        // A deliberate workload reset may pay cold setup again. Release large
        // retained halos and surfaces that the smaller workload will not draw.
        self.renderer.clear_cache();
        self.workload = workload;
        self.custom_params = false;
        self.params = workload.params();
        self.enabled = true;
        self.controls.update(cx, |controls, cx| {
            controls.params = self.params;
            controls.enabled = true;
            cx.notify();
        });
        cx.notify();
    }
    fn stress_control(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        selected: bool,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .px(px(7.))
            .h(px(23.))
            .flex()
            .items_center()
            .text_size(px(9.))
            .font_weight(FontWeight::SEMIBOLD)
            .border_1()
            .border_color(rgb(if selected {
                0x777777
            } else if self.dark {
                0x333333
            } else {
                0xdddddd
            }))
            .bg(rgb(if self.dark {
                if selected { 0x303030 } else { 0x161616 }
            } else if selected {
                0xdddddd
            } else {
                0xfafafa
            }))
            .cursor_pointer()
            .child(label.into())
    }
    fn panel(&self) -> u32 {
        if self.dark { 0x111111 } else { 0xffffff }
    }
    fn muted(&self) -> u32 {
        if self.dark { 0x999999 } else { 0x555555 }
    }
    fn material(&self, id: u64, shape: Shape, mut params: GlassParams) -> AnyElement {
        params.tint_color = [if self.dark { 0.035 } else { 0.985 }; 3];
        if self.enabled {
            gpui_glass::glass_layer(self.renderer.clone(), id, params, shape)
                .absolute()
                .inset_0()
                .into_any_element()
        } else {
            div()
                .absolute()
                .inset_0()
                .rounded(px(22.))
                .bg(rgba(if self.dark { 0x222222dd } else { 0xffffffdd }))
                .into_any_element()
        }
    }
    fn button(&self, id: u64, label: impl Into<SharedString>, w: f32) -> Stateful<Div> {
        div()
            .id(ElementId::Name(format!("prism-{id}").into()))
            .relative()
            .w(px(w))
            .h(px(30.))
            .cursor_pointer()
            .child(self.material(id, Shape::Capsule, self.params))
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label.into()),
            )
    }
    fn reflective(&self, id: u64, color: u32) -> AnyElement {
        if !self.enabled || self.workload == Workload::Realistic {
            return div().absolute().inset_0().bg(rgb(color)).into_any_element();
        }
        let base = [
            ((color >> 16) & 255u32) as f32 / 255.,
            ((color >> 8) & 255u32) as f32 / 255.,
            (color & 255u32) as f32 / 255.,
        ];
        self.material(id, Shape::Rectangle, self.params.reflection_only(base))
    }
    fn frame(&self, id: u64, title: &str) -> Div {
        div()
            .relative()
            .p(px(7.))
            .bg(rgb(self.panel()))
            .border_1()
            .border_color(rgb(if self.dark { 0x303030 } else { 0xdddddd }))
            .child(
                div()
                    .absolute()
                    .top(px(1.))
                    .bottom(px(1.))
                    .left(px(1.))
                    .right(px(1.))
                    .child(self.reflective(id, self.panel())),
            )
            .child(
                text(title.to_owned(), 9.)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(self.muted())),
            )
    }
    fn ticker(&self) -> Div {
        let mut row = div().flex().gap(px(4.)).h(px(48.));
        for i in 0..6 {
            let v = (self.phase + i as f32).sin();
            row = row.child(
                self.frame(
                    100 + i as u64,
                    ["BTC", "ETH", "SOL", "AVAX", "LINK", "SUI"][i],
                )
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .child(text(
                            format!(
                                "{:.2}",
                                [67420., 3540., 142., 36., 18., 1.8][i] * (1. + v * 0.01)
                            ),
                            12.,
                        ))
                        .child(
                            text(format!("{:+.2}%", v * 5.), 10.).text_color(rgb(if v > 0. {
                                0x00e890
                            } else {
                                0xff395d
                            })),
                        ),
                ),
            );
        }
        row
    }
    fn book(&self, trades: bool) -> Div {
        let mut panel = self
            .frame(
                if trades { 107 } else { 106 },
                if trades {
                    "RECENT TRADES"
                } else {
                    "ORDER BOOK · USD"
                },
            )
            .w(px(if trades { 148. } else { 182. }))
            .flex_shrink_0()
            .overflow_hidden()
            .child(
                text(
                    if trades {
                        "Price       Size       Time"
                    } else {
                        "Price        Amount       Total"
                    },
                    8.,
                )
                .mt_1()
                .text_color(rgb(self.muted())),
            );
        for i in 0..23 {
            let bid = if trades {
                (i as f32 + self.phase * 0.5).sin() > 0.
            } else {
                i > 11
            };
            let color = if bid { 0x00ee88 } else { 0xff3355 };
            let amount = 0.01 + ((self.phase * 1.3 + i as f32 * 3.7).sin() + 1.) * 1.8;
            panel = panel.child(
                div()
                    .relative()
                    .h(px(10.8))
                    .text_size(px(8.))
                    .font_family("Menlo")
                    .child(
                        div()
                            .absolute()
                            .right_0()
                            .top_0()
                            .bottom_0()
                            .w(relative(amount / 4.))
                            .bg(rgba((color << 8) | 50)),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .justify_between()
                            .child(
                                text(
                                    format!(
                                        "{:.1}",
                                        67420. + (i as f32 - 11.) * 2. + self.phase.sin() * 4.
                                    ),
                                    8.,
                                )
                                .text_color(rgb(color)),
                            )
                            .child(text(format!("{amount:.3}"), 8.))
                            .child(text(
                                if trades {
                                    format!("14:32:{:02}", (i + 30) % 60)
                                } else {
                                    format!("{:.2}", amount * 4.7)
                                },
                                8.,
                            )),
                    ),
            );
        }
        panel
    }
    fn heatmap(&self) -> Div {
        let mut panel = self
            .frame(108, "MARKET HEATMAP · 24H CHANGE")
            .flex_1()
            .min_w_0();
        for row in 0..4 {
            let mut line = div().flex().gap(px(3.)).mt(px(3.));
            for col in 0..5 {
                let i = row * 5 + col;
                let v = (self.phase * 0.65 + i as f32 * 1.7).sin();
                let c = if v > 0. { 0x00a66a } else { 0xca2447 };
                let alpha = (110. + v.abs() * 140.) / 255.;
                let base = self.panel();
                let channel = |shift: u32| {
                    (((c >> shift) & 255u32) as f32 * alpha
                        + ((base >> shift) & 255u32) as f32 * (1. - alpha))
                        .round() as u32
                };
                let surface = (channel(16) << 16) | (channel(8) << 8) | channel(0);
                line = line.child(
                    div()
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .h(px(30.))
                        .px(px(4.))
                        .child(self.reflective(200 + i as u64, surface))
                        .text_color(rgb(if self.dark { 0xffffff } else { 0x111111 }))
                        .text_size(px(8.))
                        .child(
                            [
                                "BTC", "ETH", "SOL", "AVAX", "LINK", "SUI", "OP", "ARB", "NEAR",
                                "UNI", "INJ", "AAVE", "DOT", "SEI", "TIA", "ATOM", "ADA", "APT",
                                "DOGE", "PEPE",
                            ][i],
                        )
                        .child(format!("{:+.2}%", v * 12.)),
                );
            }
            panel = panel.child(line);
        }
        panel
    }
    fn positions(&self) -> Div {
        let mut panel = self
            .frame(109, "OPEN POSITIONS · PAPER ACCOUNT")
            .flex_1()
            .min_w_0()
            .child(
                text("Contract      Size         Entry        PnL", 8.)
                    .mt_2()
                    .text_color(rgb(self.muted())),
            );
        for i in 0..8 {
            let v = (self.phase * 0.7 + i as f32).sin();
            panel = panel.child(
                div()
                    .h(px(16.))
                    .border_b_1()
                    .border_color(rgba(0x88888822))
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(text(
                        [
                            "BTC-PERP",
                            "ETH-PERP",
                            "SOL-PERP",
                            "AVAX-PERP",
                            "LINK-PERP",
                            "OP-PERP",
                            "SUI-PERP",
                            "ARB-PERP",
                        ][i],
                        9.,
                    ))
                    .child(text(format!("{:.3}", 0.2 + i as f32 * 0.17), 9.))
                    .child(text(format!("{:.2}", 1420. / (i + 1) as f32), 9.))
                    .child(
                        text(format!("{:+.2}", v * 240.), 9.).text_color(rgb(if v > 0. {
                            0x00dd88
                        } else {
                            0xff3355
                        })),
                    ),
            );
        }
        panel
    }
    fn market_table(&self) -> Div {
        let mut panel = self.frame(110, "ALL MARKETS · LIVE QUOTES / DEPTH / FUNDING");
        panel=panel.child(text("CONTRACT              PRICE          24H CHANGE          VOLUME          OPEN INTEREST          FUNDING",9.).mt_2().text_color(rgb(self.muted())));
        for i in 0..12 {
            let v = (self.phase + i as f32).sin();
            panel = panel.child(
                div()
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgba(0x88888822))
                    .child(
                        text(
                            [
                                "BTC / USD",
                                "ETH / USD",
                                "SOL / USD",
                                "AVAX / USD",
                                "LINK / USD",
                                "SUI / USD",
                                "OP / USD",
                                "ARB / USD",
                                "NEAR / USD",
                                "UNI / USD",
                                "AAVE / USD",
                                "DOGE / USD",
                            ][i],
                            10.,
                        )
                        .w(px(105.)),
                    )
                    .child(text(
                        format!("{:.4}", 67420. / (i + 1) as f32 + v * 4.),
                        10.,
                    ))
                    .child(
                        text(format!("{:+.2}%", v * 8.), 10.).text_color(rgb(if v > 0. {
                            0x00dd88
                        } else {
                            0xff3355
                        })),
                    )
                    .child(text(format!("${:.1}M", 92. + v * 20.), 10.))
                    .child(text(format!("${:.1}M", 240. + v * 30.), 10.))
                    .child(text(format!("{:+.4}%", v * 0.02), 10.).text_color(rgb(COLORS[4]))),
            );
        }
        panel
    }
    fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        let mut nav = div()
            .text_size(px(10.))
            .flex()
            .flex_col()
            .gap_2()
            .mt(px(16.));
        for (i, (icon, label)) in [
            ("◈", "Overview"),
            ("⌁", "Markets"),
            ("⇄", "Exchange"),
            ("◎", "Portfolio"),
            ("◷", "Activity"),
        ]
        .into_iter()
        .enumerate()
        {
            nav = nav.child(
                div()
                    .id(ElementId::Name(format!("nav-{i}").into()))
                    .px_2()
                    .py_2()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .bg(if i == self.selected {
                        rgba(0x88888830)
                    } else {
                        rgba(0)
                    })
                    .text_color(rgb(if i == self.selected {
                        if self.dark { 0xffffff } else { 0x111111 }
                    } else {
                        self.muted()
                    }))
                    .child(format!("{icon}   {label}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = i;
                        this.phase += 0.8;
                        cx.notify();
                    })),
            );
        }
        let sidebar_params =
            self.params
                .reflection_only(if self.dark { [0.055; 3] } else { [0.96; 3] });
        div()
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .w(px(124.))
            .child(self.material(1, Shape::Rectangle, sidebar_params))
            .child(
                div()
                    .relative()
                    .size_full()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .child(
                        text("◈ PRISM", 17.)
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(if self.dark { 0xffffff } else { 0x111111 })),
                    )
                    .child(
                        text("REFLECTION STRESS TEST", 8.)
                            .mt_2()
                            .text_color(rgb(self.muted())),
                    )
                    .child(nav)
                    .child(div().flex_1())
                    .child(tag(
                        if self.live {
                            "● Workload running"
                        } else {
                            "Ⅱ Workload paused"
                        },
                        COLORS[0],
                    ))
                    .child(
                        text("Synthetic data", 11.)
                            .mt_4()
                            .text_color(rgb(self.muted())),
                    )
                    .child(text("$128,430.80", 13.).mt_1())
                    .child(
                        text(
                            "Deliberately excessive surfaces. Not an appearance reference.",
                            9.,
                        )
                        .mt_2()
                        .text_color(rgb(self.muted())),
                    ),
            )
    }
    fn morph_row(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div().relative().flex().gap_3().w_full().h(px(64.)).child(
            div()
                .absolute()
                .inset_0()
                .child(chart(self.phase * 1.4, 0, 2)),
        );
        for i in 0..3 {
            let v = self.morphs[i].value;
            let mut p = self.params;
            p.dynamic_shape = true;
            p.shape_scale = match i {
                0 => [0.12 + 0.5 * v, 0.62 + 0.22 * v],
                1 => [0.62 + 0.25 * v, 0.85 - 0.34 * v],
                _ => [0.58 - 0.45 * v, 0.54 + 0.32 * v],
            };
            if self.preview != 0 {
                p.surface = Some([if self.preview == 2 { 0.15 } else { 0.94 }; 3]);
            }
            let label = match i {
                0 => {
                    if v > 0.5 {
                        "↗  Quick trade"
                    } else {
                        "+"
                    }
                }
                1 => {
                    if v > 0.5 {
                        "●  Watching SOL"
                    } else {
                        "◎ Watch"
                    }
                }
                _ => {
                    if v > 0.5 {
                        "✓"
                    } else {
                        "⇄ Rebalance"
                    }
                }
            };
            row = row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .h(px(60.))
                    .child(self.material(40 + i as u64, self.shape, p))
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .id(ElementId::Name(format!("morph-{i}").into()))
                                    .w(relative(p.shape_scale[0]))
                                    .h(relative(p.shape_scale[1]))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .text_size(px(10.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.morphs[i].toggle();
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }
        row
    }
    fn popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let amount = self.pop.value.clamp(0., 1.);
        let mut p = motion::materialise(self.params, amount);
        p.tint_color = [if self.dark { 0.035 } else { 0.985 }; 3];
        // Fixed geometry: only the optical parameters and foreground fade animate.
        static CONTOUR: std::sync::OnceLock<Arc<gpui_glass::GlassContour>> =
            std::sync::OnceLock::new();
        let contour = CONTOUR
            .get_or_init(|| {
                Arc::new(
                    gpui_glass::GlassContour::new(
                        gpui_smooth::SmoothShape::rounded_rect(16., 1.)
                            .outline(310., 256.)
                            .into_iter()
                            .map(|[x, y]| [(x / 310.).clamp(0., 1.), (y / 256.).clamp(0., 1.)])
                            .collect(),
                    )
                    .expect("valid popover contour"),
                )
            })
            .clone();
        let renderer = self.renderer.clone();
        let background = if self.enabled {
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    gpui_glass::GlassDraw {
                        renderer,
                        id: 70,
                        params: p,
                        shape: Shape::Rounded,
                        scale: window.scale_factor(),
                        contour: Some(contour),
                    }
                    .paint_in(window, bounds);
                },
            )
            .absolute()
            .inset_0()
            .into_any_element()
        } else {
            div()
                .absolute()
                .inset_0()
                .rounded(px(16.))
                .bg(rgb(self.panel()))
                .opacity(amount)
                .into_any_element()
        };
        div()
            .absolute()
            .top(px(90.))
            .right(px(22.))
            .w(px(310.))
            .h(px(256.))
            .child(background)
            .child(
                div()
                    .relative()
                    .size_full()
                    .p(px(26.))
                    .opacity(amount)
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(
                                text(
                                    if self.pop_kind == 0 {
                                        "Market view"
                                    } else {
                                        "Quick order"
                                    },
                                    18.,
                                )
                                .font_weight(FontWeight::BOLD),
                            )
                            .child(
                                div()
                                    .id("close-popover")
                                    .cursor_pointer()
                                    .child("×")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.pop.target = 0.;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        text(
                            if self.pop_kind == 0 {
                                "Choose a simulated market"
                            } else {
                                "Paper account · nothing is submitted"
                            },
                            10.,
                        )
                        .mt_2()
                        .text_color(rgb(self.muted())),
                    )
                    .children(
                        [
                            ("BTC / USD", COLORS[3]),
                            ("ETH / USD", COLORS[1]),
                            ("SOL / USD", COLORS[0]),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (label, color))| {
                            div()
                                .id(ElementId::Name(format!("market-option-{i}").into()))
                                .mt_3()
                                .p_2()
                                .rounded(px(9.))
                                .bg(rgba((color << 8) | 22))
                                .cursor_pointer()
                                .text_color(rgb(color))
                                .child(format!("●  {label}                    ↗"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected = i;
                                    this.pop.target = 0.;
                                    this.phase += 1.;
                                    cx.notify();
                                }))
                        }),
                    ),
            )
            .into_any_element()
    }
}
impl Render for Market {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32();
        self.last = now;
        let reduced = cx.reduce_motion();
        if self.animating(reduced) && dt < 0.2 {
            self.frame_ms = self.frame_ms * 0.95 + dt * 1000. * 0.05;
        }
        if self.live && !reduced {
            self.phase += dt.min(0.05) * 1.25;
        }
        for s in &mut self.morphs {
            s.step(dt, reduced);
        }
        self.pop.step(dt, reduced);
        if self.animating(reduced) && !self.tick_pending {
            self.tick_pending = true;
            Self::request_tick(cx.weak_entity(), window);
        }
        let phase = self.phase;
        let stats = self.renderer.stats();
        let workspace = div()
            .absolute()
            .left(px(124.))
            .right(px(280.))
            .top_0()
            .bottom_0()
            .overflow_hidden()
            .child(
                div()
                    .id("market-scroll")
                    .absolute()
                    .inset_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .px(px(8.))
                            .pt(px(82.))
                            .pb(px(10.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(self.ticker())
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .h(px(290.))
                                    .child(
                                        self.frame(111, "BTC / USD   ·   PERPETUAL   ·   1m")
                                            .flex_1()
                                            .min_w_0()
                                            .relative()
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(px(6.))
                                                    .right(px(6.))
                                                    .top(px(28.))
                                                    .bottom(px(20.))
                                                    .child(chart(phase, COLORS[0], 1)),
                                            )
                                            .child(
                                                div().absolute().left(px(12.)).top(px(40.)).child(
                                                    self.button(
                                                        80,
                                                        format!(
                                                            "{:.2}   +4.28%   VOL 1.83B",
                                                            67420. + phase.sin() * 220.
                                                        ),
                                                        248.,
                                                    )
                                                    .text_color(rgb(COLORS[0])),
                                                ),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(px(18.))
                                                    .bottom(px(40.))
                                                    .child(
                                                        self.button(
                                                            81,
                                                            "● LIVE   ·   Spread 0.01%",
                                                            144.,
                                                        )
                                                        .text_color(rgb(COLORS[0])),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .right(px(28.))
                                                    .top(px(144.))
                                                    .child(
                                                        self.button(20, "Trade BTC ↗", 90.)
                                                            .on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.pop_kind = 1;
                                                                    this.pop.target = 1.;
                                                                    cx.notify();
                                                                },
                                                            )),
                                                    ),
                                            ),
                                    )
                                    .child(self.book(false))
                                    .child(self.book(true)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .h(px(174.))
                                    .child(self.heatmap())
                                    .child(self.positions()),
                            )
                            .child(self.morph_row(cx))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .h(px(130.))
                                    .children((0..3).map(|i| {
                                        self.frame(
                                            112 + i as u64,
                                            [
                                                "ETH / USD  ·  5m",
                                                "SOL / USD  ·  1m",
                                                "RELATIVE MOMENTUM",
                                            ][i],
                                        )
                                        .flex_1()
                                        .min_w_0()
                                        .relative()
                                        .child(
                                            div()
                                                .absolute()
                                                .left(px(5.))
                                                .right(px(5.))
                                                .top(px(25.))
                                                .bottom(px(6.))
                                                .child(chart(
                                                    phase + i as f32,
                                                    COLORS[i],
                                                    if i == 2 { 2 } else { 0 },
                                                )),
                                        )
                                        .child(
                                            div().absolute().right(px(12.)).top(px(60.)).child(
                                                self.button(12 + i as u64, "↗", 28.).on_click(
                                                    cx.listener(|this, _, _, cx| {
                                                        this.pop_kind = 1;
                                                        this.pop.target = 1.;
                                                        cx.notify();
                                                    }),
                                                ),
                                            ),
                                        )
                                    })),
                            )
                            .child(self.market_table()),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.))
                    .right(px(8.))
                    .top(px(8.))
                    .flex()
                    .justify_between()
                    .child(self.button(30, "◈  Markets ⌄", 116.).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.pop_kind = 0;
                            this.pop.toggle();
                            cx.notify();
                        },
                    )))
                    .child(
                        div()
                            .flex()
                            .gap(px(5.))
                            .child(
                                self.button(34, "SYNTHETIC WORKLOAD", 128.)
                                    .text_color(rgb(COLORS[0])),
                            )
                            .child(
                                self.button(31, if self.live { "Ⅱ Pause" } else { "▶ Run" }, 66.)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.live = !this.live;
                                        this.last = Instant::now();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                self.button(32, if self.dark { "☀ Light" } else { "☾ Dark" }, 66.)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dark = !this.dark;
                                        let dark = this.dark;
                                        this.controls.update(cx, |c, cx| {
                                            c.dark = dark;
                                            cx.notify();
                                        });
                                        cx.notify();
                                    })),
                            )
                            .child(self.button(33, "+ Order", 70.).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.pop_kind = 1;
                                    this.pop.toggle();
                                    cx.notify();
                                },
                            ))),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.))
                    .right(px(8.))
                    .top(px(44.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(text("LOAD", 9.).text_color(rgb(self.muted())))
                    .children(
                        [
                            ("load-realistic", Workload::Realistic),
                            ("load-prism", Workload::Prism),
                            ("load-cranked", Workload::Cranked),
                        ]
                        .into_iter()
                        .map(|(id, workload)| {
                            self.stress_control(id, workload.label(), self.workload == workload)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_workload(workload, cx)
                                }))
                        }),
                    )
                    .child(
                        self.stress_control(
                            "frame-target",
                            format!("Target {} fps", self.target_hz),
                            false,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.target_hz = if this.target_hz == 60 { 120 } else { 60 };
                            this.next_tick = Instant::now();
                            cx.notify();
                        })),
                    )
                    .child(
                        text(
                            if self.custom_params {
                                "Custom settings"
                            } else if self.workload == Workload::Realistic {
                                "Floating controls + rail"
                            } else if self.workload == Workload::Cranked {
                                "All tiles · 10× reflections"
                            } else {
                                "All tiles reflective"
                            },
                            9.,
                        )
                        .text_color(rgb(self.muted())),
                    ),
            )
            .when(
                self.pop.target > 0. || self.pop.value > 0.001 || self.pop.active(),
                |d| d.child(self.popover(cx)),
            );
        div()
            .size_full()
            .relative()
            .font_family(".AppleSystemUIFont")
            .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
            .text_color(rgb(if self.dark { 0xf5f5f5 } else { 0x111111 }))
            .bg(rgb(if self.dark { 0x080808 } else { 0xffffff }))
            .child(workspace)
            .child(self.sidebar(cx))
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(280.))
                    .bg(rgb(self.panel()))
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(text("STRESS INSPECTOR", 13.).font_weight(FontWeight::BOLD))
                    .child(
                        text(
                            format!(
                                "{:.0} fps · {:.1} ms render cadence",
                                1000. / self.frame_ms.max(0.001), self.frame_ms
                            ),
                            10.,
                        )
                        .text_color(rgb(self.muted())),
                    )
                    .child(
                        text(
                            if self.animating(reduced) {
                                "Cadence includes CPU work and scheduling; not GPU time."
                            } else {
                                "Paused / settled: no animation redraws. Cadence above is the last running sample."
                            },
                            9.,
                        ).text_color(rgb(self.muted())),
                    )
                    .child(
                        text(format!("{} SDF rebuilds · full-resolution output", stats.dynamic_atlas_builds), 9.)
                            .text_color(rgb(self.muted())),
                    )
                    .child(
                        div()
                            .id("material-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(self.controls.clone()),
                    ),
            )
    }
}
fn main() -> anyhow::Result<()> {
    let Some(config) = LaunchConfig::from_args(std::env::args().skip(1))? else {
        return Ok(());
    };
    env_logger::init();
    gpui_platform::application().run(move |cx: &mut App| {
        #[cfg(target_os = "macos")]
        cx.set_reduce_motion(
            objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion(),
        );
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1440.), px(940.)), cx);
        cx.open_window(
            WindowOptions::new()
                .titlebar(Some(TitlebarOptions {
                    title: Some("Prism — Glass Stress Test".into()),
                    ..Default::default()
                }))
                .window_bounds(Some(WindowBounds::Windowed(bounds)))
                .window_min_size(Some(size(px(1180.), px(740.)))),
            move |_, cx| {
                let params = config.workload.params();
                let controls = cx.new(|_| {
                    let mut c = GlassControls::new(params);
                    c.dark = true;
                    c.shape = Shape::Capsule;
                    c
                });
                cx.new(move |cx| {
                    cx.subscribe(
                        &controls,
                        |this: &mut Market, _, e: &ControlsChanged, cx| {
                            this.params = e.params;
                            this.custom_params = true;
                            if this.enabled && !e.enabled {
                                this.renderer.clear_cache();
                            }
                            this.enabled = e.enabled;
                            this.shape = e.shape;
                            this.preview = e.lighting_preview;
                            cx.notify();
                        },
                    )
                    .detach();
                    Market {
                        renderer: Arc::new(GlassRenderer::default()),
                        controls,
                        params,
                        enabled: true,
                        shape: Shape::Capsule,
                        preview: 0,
                        dark: true,
                        live: config.live,
                        phase: 0.,
                        last: Instant::now(),
                        morphs: [Spring::new(0.); 3],
                        pop: Spring::new(0.),
                        pop_kind: 0,
                        selected: 0,
                        scroll: ScrollHandle::new(),
                        frame_ms: 1000. / config.target_hz as f32,
                        workload: config.workload,
                        custom_params: false,
                        target_hz: config.target_hz,
                        next_tick: Instant::now(),
                        tick_pending: false,
                    }
                })
            },
        )
        .expect("open Prism");
        cx.activate(true);
    });
    Ok(())
}
