//! GPUI Kit's ScrollBounce, adapted to this demo's GPUI-CE build.
//! Upstream: longbridge/gpui-kit, revision 0790ad3876ebe6b72ca0bf599db7f7d1718c6b61.
//! Copyright 2024-2026 Longbridge. Apache-2.0; see LICENSES/GPUI-Kit-Apache-2.0.txt.
//! Changes: local scroll-handle adapters, GPUI-CE content-mask defaults, and native axis locking.
//! Boundary displacement only: the list keeps its clamped logical position.

use gpui::{
    AnyElement, App, AppContext as _, Bounds, ContentMask, DispatchPhase, Element, ElementId,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId,
    OngoingScroll, Pixels, ScrollDelta, ScrollWheelEvent, TouchPhase, Window, point, px,
};
use std::sync::Arc;
use std::time::Instant;
use std::{cell::RefCell, rc::Rc, time::Duration};
#[path = "scroll_deform.rs"]
pub mod deform;
pub use deform::{DeformationRenderer, ScrollDeformation};

// The two adapters needed by the demo and the upstream interaction tests.
// Derived from GPUI Kit's scrollbar.rs; no dependency on the full component kit.
pub trait ScrollbarHandle: 'static {
    fn viewport_bounds(&self) -> Bounds<Pixels>;
    fn offset(&self) -> gpui::Point<Pixels>;
    fn set_offset(&self, offset: gpui::Point<Pixels>);
    fn content_size(&self) -> gpui::Size<Pixels>;
}
impl ScrollbarHandle for gpui::ScrollHandle {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.bounds()
    }
    fn offset(&self) -> gpui::Point<Pixels> {
        self.offset()
    }
    fn set_offset(&self, offset: gpui::Point<Pixels>) {
        self.set_offset(offset);
    }
    fn content_size(&self) -> gpui::Size<Pixels> {
        (self.max_offset() + self.bounds().size.into()).into()
    }
}
impl ScrollbarHandle for gpui::ListState {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.viewport_bounds()
    }
    fn offset(&self) -> gpui::Point<Pixels> {
        self.scroll_px_offset_for_scrollbar()
    }
    fn set_offset(&self, offset: gpui::Point<Pixels>) {
        self.set_offset_from_scrollbar(offset);
    }
    fn content_size(&self) -> gpui::Size<Pixels> {
        self.viewport_bounds().size + self.max_offset_for_scrollbar().into()
    }
}

/// Motion tokens for [`ScrollBounce`]: how far a drag stretches the viewport
/// past an edge, and how quickly a released edge returns.
///
/// Base plays the stretch and the return; the feel belongs to the caller.
/// The default is tuned to feel like a `UIScrollView` bounce.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBounceMotion {
    tracking: f32,
    response: Duration,
    damping_ratio: f32,
}

impl Default for ScrollBounceMotion {
    /// Tuned to feel like a `UIScrollView` bounce; these are not UIKit constants.
    fn default() -> Self {
        Self {
            tracking: 0.55,
            response: Duration::from_millis(524),
            damping_ratio: 1.,
        }
    }
}

impl ScrollBounceMotion {
    /// Fraction of finger travel the stretched edge follows at first.
    ///
    /// The edge follows less and less as it approaches the viewport height,
    /// which it never reaches. Tracking is independent of the return, so
    /// slowing the return does not change how the finger feels.
    ///
    /// # Panics
    ///
    /// Panics when `tracking` is not finite or not positive.
    pub fn with_tracking(mut self, tracking: f32) -> Self {
        assert!(
            tracking.is_finite() && tracking > 0.,
            "scroll bounce tracking must be finite and positive"
        );
        self.tracking = tracking;
        self
    }

    /// Time scale of the return once the finger lifts.
    ///
    /// Like GPUI Kit's spring response: the period one full oscillation would
    /// take without damping, which is the scale the
    /// return is felt at rather than the moment it stops. The return is
    /// critically damped by default, so it never crosses the edge. A zero response snaps
    /// the edge back on the spot.
    pub fn with_response(mut self, response: Duration) -> Self {
        self.response = response;
        self
    }

    /// Fraction of finger travel the stretched edge follows at first.
    pub fn tracking(&self) -> f32 {
        self.tracking
    }

    /// Time scale of the return once the finger lifts.
    pub fn response(&self) -> Duration {
        self.response
    }

    /// 1 is critical damping; below 1 oscillates, above 1 returns more slowly.
    pub fn with_damping_ratio(mut self, ratio: f32) -> Self {
        assert!(ratio.is_finite() && ratio > 0.);
        self.damping_ratio = ratio;
        self
    }

    pub fn damping_ratio(&self) -> f32 {
        self.damping_ratio
    }

    /// Undamped angular frequency of the return, or `None` when it snaps.
    fn omega(&self) -> Option<f32> {
        let seconds = self.response.as_secs_f32();
        (seconds > 0.).then(|| std::f32::consts::TAU / seconds)
    }
}

/// Advanced edge/gesture constants. Defaults preserve the original behavior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBounceTuning {
    pub extent_ratio: f32,
    pub settle_position: f32,
    pub settle_velocity: f32,
    pub catch_drag_slop: f32,
    pub momentum_gap: Duration,
    pub residual_epsilon: f32,
    pub inverse_epsilon: f32,
    pub axis_lock: bool,
    pub axis_gap: Duration,
    pub axis_unlock_ratio: f32,
    pub axis_unlock_distance: f32,
    /// Scale unused momentum distance at an edge separately from finger tracking.
    pub momentum_gain: f32,
    /// Fraction of incoming momentum velocity passed into the return spring.
    pub momentum_transfer: f32,
    /// Fraction of finger velocity retained on release.
    pub release_transfer: f32,
    /// Absolute displacement ceiling in pixels; zero uses the viewport ceiling.
    pub max_bounce: f32,
}

impl Default for ScrollBounceTuning {
    fn default() -> Self {
        Self {
            extent_ratio: 1.,
            settle_position: 0.1,
            settle_velocity: 1.,
            catch_drag_slop: CATCH_DRAG_SLOP,
            momentum_gap: MOMENTUM_GAP,
            residual_epsilon: 0.01,
            inverse_epsilon: 0.01,
            axis_lock: true,
            axis_gap: Duration::from_millis(28),
            axis_unlock_ratio: 1.9,
            axis_unlock_distance: 6.,
            momentum_gain: 1.,
            momentum_transfer: 0.,
            release_transfer: 0.,
            max_bounce: 0.,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollBounceSnapshot {
    pub displacement: f32,
    pub velocity: f32,
    pub dragging: bool,
    pub suppress_momentum: bool,
}

enum DebugCommand {
    Reset,
    Pull(f32, f32),
    Begin,
    Release,
    Momentum(f32),
}

/// Optional playground instrumentation; absent in normal application use.
#[derive(Clone, Default)]
pub struct ScrollBounceDebug(Rc<RefCell<DebugData>>);

#[derive(Default)]
struct DebugData {
    commands: Vec<DebugCommand>,
    snapshot: ScrollBounceSnapshot,
}

impl ScrollBounceDebug {
    pub fn snapshot(&self) -> ScrollBounceSnapshot {
        self.0.borrow().snapshot
    }
    pub fn reset(&self) {
        self.0.borrow_mut().commands.push(DebugCommand::Reset);
    }
    pub fn begin(&self) {
        self.0.borrow_mut().commands.push(DebugCommand::Begin);
    }
    pub fn pull(&self, pixels: f32) {
        if pixels.is_finite() {
            self.0
                .borrow_mut()
                .commands
                .push(DebugCommand::Pull(pixels, 0.));
        }
    }
    pub fn pull_with_velocity(&self, pixels: f32, velocity: f32) {
        if pixels.is_finite() && velocity.is_finite() {
            self.0
                .borrow_mut()
                .commands
                .push(DebugCommand::Pull(pixels, velocity));
        }
    }
    /// A repeatable 60 Hz momentum packet arriving at an edge, in px/s.
    pub fn momentum(&self, velocity: f32) {
        if velocity.is_finite() {
            self.0
                .borrow_mut()
                .commands
                .push(DebugCommand::Momentum(velocity));
        }
    }
    pub fn release(&self) {
        self.0.borrow_mut().commands.push(DebugCommand::Release);
    }
}

/// Adds vertical touch overscroll to an existing scroll viewport.
///
/// The child owns layout, content, and ordinary scrolling; `handle` must be the
/// child's scroll handle. Only unused vertical deltas stretch the viewport.
/// The stable `id` owns gesture and spring state. Change it when replacing the
/// document. Put fixed chrome (scrollbars, toolbars) outside this wrapper.
///
/// Enabled by default on iOS and Android. Other platforms pass through unless
/// explicitly enabled; their input must emit `Ended` at finger release, before momentum.
/// Reduced motion disables displacement. Keyboard, focus, and line-wheel input
/// remain owned by the child. No colors, padding, or dimensions are imposed.
pub struct ScrollBounce {
    id: ElementId,
    handle: Rc<dyn ScrollbarHandle>,
    child: AnyElement,
    enabled: bool,
    motion: ScrollBounceMotion,
    tuning: ScrollBounceTuning,
    debug: Option<ScrollBounceDebug>,
    debug_time_scale: f32,
    deformation: Option<(Arc<DeformationRenderer>, ScrollDeformation)>,
    on_scroll: Option<ScrollCallback>,
}

type ScrollCallback = Rc<dyn Fn(&mut Window, &mut App)>;

impl ScrollBounce {
    pub fn new<H: ScrollbarHandle + Clone>(
        id: impl Into<ElementId>,
        handle: &H,
        child: impl IntoElement,
    ) -> Self {
        Self {
            id: id.into(),
            handle: Rc::new(handle.clone()),
            child: child.into_any_element(),
            enabled: cfg!(any(target_os = "ios", target_os = "android")),
            motion: ScrollBounceMotion::default(),
            tuning: ScrollBounceTuning::default(),
            debug: None,
            debug_time_scale: 1.,
            deformation: None,
            on_scroll: None,
        }
    }

    /// Opt in on a platform with compatible touch phase semantics.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set how far a drag stretches past an edge and how the edge returns.
    pub fn motion(mut self, motion: ScrollBounceMotion) -> Self {
        self.motion = motion;
        self
    }

    /// Scale the painted viewport with the bounce, without changing logical layout.
    pub fn deformation(
        mut self,
        renderer: Arc<DeformationRenderer>,
        params: ScrollDeformation,
    ) -> Self {
        assert!(params.along_axis.is_finite() && params.cross_axis.is_finite());
        assert!((0.0..1.0).contains(&params.max_deformation));
        assert!((0.0..=1.0).contains(&params.translation));
        assert!(params.exponent.is_finite() && params.exponent > 0.);
        self.deformation = Some((renderer, params));
        self
    }

    pub fn tuning(mut self, tuning: ScrollBounceTuning) -> Self {
        assert!(tuning.extent_ratio.is_finite() && tuning.extent_ratio > 0.);
        assert!(
            [
                tuning.settle_position,
                tuning.settle_velocity,
                tuning.inverse_epsilon
            ]
            .into_iter()
            .all(|v| v.is_finite() && v > 0.)
        );
        assert!(
            [
                tuning.catch_drag_slop,
                tuning.residual_epsilon,
                tuning.axis_unlock_distance,
                tuning.momentum_gain,
                tuning.momentum_transfer,
                tuning.release_transfer,
                tuning.max_bounce
            ]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.)
        );
        assert!(tuning.axis_unlock_ratio.is_finite() && tuning.axis_unlock_ratio >= 1.);
        self.tuning = tuning;
        self
    }

    pub fn debug(mut self, debug: ScrollBounceDebug) -> Self {
        self.debug = Some(debug);
        self
    }

    /// Slow-motion return for the debug playground only.
    pub fn debug_time_scale(mut self, scale: f32) -> Self {
        assert!(scale.is_finite() && scale > 0.);
        self.debug_time_scale = scale;
        self
    }

    /// Observe a logical scroll performed when a reverse drag leaves the stretched
    /// region. Runs after the handle update, with no internal state borrowed.
    /// Ordinary child scrolling continues to use the child's own notifications.
    pub fn on_scroll(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_scroll = Some(Rc::new(handler));
        self
    }
}

#[derive(Default)]
struct State {
    physics: Physics,
    sampled_at: Option<Instant>,
    ongoing_scroll: OngoingScroll,
    tuned_axis: TunedAxis,
    short_drag_distance: Option<f32>,
    last_wheel_at: Option<Instant>,
    input_velocity: f32,
    /// Sign of the stream being suppressed, when it is known to be momentum
    /// that cannot reverse. `None` suppresses both directions.
    suppressed_direction: Option<f32>,
}

/// Tunable counterpart of GPUI's OngoingScroll; the original is used at defaults.
#[derive(Default)]
struct TunedAxis {
    at: Option<Instant>,
    vertical: Option<bool>,
}
impl TunedAxis {
    fn filter(
        &mut self,
        delta: &mut gpui::Point<Pixels>,
        phase: TouchPhase,
        tuning: ScrollBounceTuning,
    ) {
        if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            *self = Self::default();
            return;
        }
        let x = delta.x.abs().as_f32();
        let y = delta.y.abs().as_f32();
        if x == 0. && y == 0. {
            if phase == TouchPhase::Started {
                *self = Self::default();
            }
            return;
        }
        let now = Instant::now();
        if phase == TouchPhase::Started
            || self
                .at
                .is_none_or(|at| now.duration_since(at) >= tuning.axis_gap)
        {
            self.vertical = Some(x <= y);
        } else if x.max(y) >= tuning.axis_unlock_distance {
            if matches!(self.vertical, Some(true)) && x > y && x >= y * tuning.axis_unlock_ratio
                || matches!(self.vertical, Some(false))
                    && y > x
                    && y >= x * tuning.axis_unlock_ratio
            {
                self.vertical = None;
            }
        }
        self.at = Some(now);
        match self.vertical {
            Some(true) => delta.x = px(0.),
            Some(false) => delta.y = px(0.),
            None => {}
        }
    }
}

impl State {
    fn release_input(&mut self, from_rest: bool, input_velocity: f32) {
        let gain = if from_rest {
            self.physics.tuning.momentum_transfer
        } else {
            self.physics.tuning.release_transfer
        };
        let tracking = self.physics.motion.tracking;
        let d = self.physics.extent.max(1.);
        let derivative = tracking / (1. + tracking * self.physics.position.abs() / d).powi(2);
        self.release(from_rest);
        if self.physics.position != 0. {
            self.physics.velocity = input_velocity * derivative * gain;
        }
    }
    /// Release the edge. A stretch made outside a gesture is momentum (or a
    /// phaseless wheel) hitting the edge; that stream only pushes outward,
    /// so an inward packet is a new scroll and ends the suppression. After a
    /// gesture's own release the suppressed momentum may point either way.
    fn release(&mut self, from_rest: bool) {
        self.physics.release();
        self.suppressed_direction =
            (from_rest && self.physics.suppress_momentum).then(|| self.physics.offset().signum());
    }
}

// GPUI starts a normal touch pan only after its 8 px touch slop, but a touch
// catching a fling starts at zero displacement. A short catch should stop the
// old fling rather than turn a few fast pixels into a new one.
const CATCH_DRAG_SLOP: f32 = 8.;

// Momentum arrives once per frame until it stops, so a longer silence means
// the suppressed stream has ended. Smooth-scrolling mouse drivers on macOS
// send precise deltas with no phase: they never send the `Started` that
// otherwise ends suppression, and would stay locked after one bounce.
const MOMENTUM_GAP: Duration = Duration::from_millis(250);

/// `ScrollbarHandle` has no `max_offset`; recover it from the definition
/// `content_size = viewport + max_offset`. Both dispatch phases clamp against
/// this bound and must agree on it.
fn max_scroll_extent(handle: &dyn ScrollbarHandle) -> Pixels {
    (handle.content_size().height - handle.viewport_bounds().size.height).max(px(0.))
}

#[doc(hidden)]
pub struct ScrollBouncePrepaintState {
    state: Rc<RefCell<State>>,
    hitbox: Hitbox,
}

impl IntoElement for ScrollBounce {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ScrollBounce {
    type RequestLayoutState = ();
    type PrepaintState = ScrollBouncePrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let state = window.with_element_state(
            id.expect("ScrollBounce has an id"),
            |state: Option<Rc<RefCell<State>>>, _| {
                let state = state.unwrap_or_default();
                (state.clone(), state)
            },
        );
        let offset = {
            let mut state = state.borrow_mut();
            if !self.enabled || cx.reduce_motion() {
                *state = State::default();
            }
            state.physics.motion = self.motion;
            state.physics.tuning = self.tuning;
            if let Some(debug) = &self.debug {
                let commands = std::mem::take(&mut debug.0.borrow_mut().commands);
                let commanded = !commands.is_empty();
                let bounce = self.enabled && !cx.reduce_motion();
                for command in commands {
                    match command {
                        DebugCommand::Reset => {
                            *state = State::default();
                            state.physics.motion = self.motion;
                            state.physics.tuning = self.tuning;
                        }
                        DebugCommand::Begin => {
                            state.input_velocity = 0.;
                            if bounce {
                                state.physics.begin(bounds.size.height.as_f32());
                            }
                        }
                        DebugCommand::Pull(delta, velocity) => {
                            state.input_velocity = velocity;
                            // Mouse preview uses the same rubber band and logical clamping.
                            let max = max_scroll_extent(self.handle.as_ref()).as_f32();
                            let before = self.handle.offset().y.as_f32();
                            let remaining = if state.physics.offset() != 0. {
                                state.physics.pull(delta)
                            } else {
                                delta
                            };
                            let after = (before + remaining).clamp(-max, 0.);
                            self.handle.set_offset(point(px(0.), px(after)));
                            let residual = remaining - (after - before);
                            if bounce && residual.abs() > self.tuning.residual_epsilon {
                                state.physics.pull(residual);
                            }
                        }
                        DebugCommand::Release => {
                            if bounce {
                                let velocity = state.input_velocity;
                                state.release_input(false, velocity);
                            }
                        }
                        DebugCommand::Momentum(velocity) => {
                            if bounce {
                                state.physics.begin(bounds.size.height.as_f32());
                                state
                                    .physics
                                    .pull(velocity / 60. * self.tuning.momentum_gain);
                                state.release_input(true, velocity);
                            }
                        }
                    }
                }
                if commanded {
                    state.sampled_at = Some(Instant::now());
                }
            }
            let now = Instant::now();
            let elapsed = state
                .sampled_at
                .replace(now)
                .map_or(0., |at| now.duration_since(at).as_secs_f32());
            if state.physics.step(elapsed * self.debug_time_scale) {
                window.request_animation_frame();
            }
            if let Some(debug) = &self.debug {
                debug.0.borrow_mut().snapshot = ScrollBounceSnapshot {
                    displacement: state.physics.offset(),
                    velocity: state.physics.velocity,
                    dragging: state.physics.dragging,
                    suppress_momentum: state.physics.suppress_momentum,
                };
            }
            state.physics.offset()
        };
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        window.with_content_mask(
            Some(ContentMask {
                bounds,
                ..Default::default()
            }),
            |window| {
                // The deformation pass owns translation as well as scaling, so
                // capture undeformed content under a stable viewport/hitbox.
                let translated = if self.deformation.is_some() {
                    0.
                } else {
                    offset
                };
                window.with_element_offset(point(px(0.), px(translated)), |window| {
                    self.child.prepaint(window, cx);
                });
            },
        );
        ScrollBouncePrepaintState { state, hitbox }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.enabled && !cx.reduce_motion() {
            let state = prepaint.state.clone();
            let hitbox = prepaint.hitbox.id;
            let handle = self.handle.clone();
            let view = window.current_view();
            let on_scroll = self.on_scroll.clone();
            let tuning = self.tuning;
            let mut before = 0.;
            let mut allow_end_bounce = false;
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                let ScrollDelta::Pixels(mut delta) = event.delta else {
                    return;
                };
                if !hitbox.should_handle_scroll(window) {
                    return;
                }
                let mut state = state.borrow_mut();
                // Lock the gesture to the axis it started on, so a diagonal
                // swipe cannot wobble out of the stretch from one packet to
                // the next. Both dispatch phases see the same packet, and the
                // lock gives both the same answer.
                if tuning.axis_lock {
                    let defaults = ScrollBounceTuning::default();
                    if tuning.axis_gap == defaults.axis_gap
                        && tuning.axis_unlock_ratio == defaults.axis_unlock_ratio
                        && tuning.axis_unlock_distance == defaults.axis_unlock_distance
                    {
                        state.ongoing_scroll.filter(&mut delta, event.touch_phase);
                        state.tuned_axis = TunedAxis::default();
                    } else {
                        state
                            .tuned_axis
                            .filter(&mut delta, event.touch_phase, tuning);
                        state.ongoing_scroll = OngoingScroll::default();
                    }
                } else {
                    state.tuned_axis = TunedAxis::default();
                    state.ongoing_scroll = OngoingScroll::default();
                }
                if delta.x.abs() > delta.y.abs() {
                    return;
                }
                let ended = matches!(event.touch_phase, TouchPhase::Ended | TouchPhase::Cancelled);
                let mut scrolled = false;
                let mut changed = false;
                if phase == DispatchPhase::Capture {
                    before = handle.offset().y.as_f32();
                    if event.touch_phase == TouchPhase::Started {
                        state.short_drag_distance = (delta.y == px(0.)).then_some(0.);
                        state.physics.begin(bounds.size.height.as_f32());
                    } else if let Some(distance) = state.short_drag_distance.as_mut() {
                        *distance += delta.y.as_f32().abs();
                        if *distance > tuning.catch_drag_slop {
                            state.short_drag_distance = None;
                        }
                    }
                    let suppress_short_drag_momentum = if ended {
                        state.short_drag_distance.take().is_some()
                            && event.touch_phase == TouchPhase::Ended
                    } else {
                        false
                    };
                    // The current Ended packet may still cross an edge; only
                    // momentum packets after it should be suppressed.
                    allow_end_bounce = suppress_short_drag_momentum;
                    let now = cx.background_executor().now();
                    let previous = state.last_wheel_at.replace(now);
                    let interval = previous.map_or(1. / 60., |at| {
                        now.saturating_duration_since(at).as_secs_f32()
                    });
                    let paused =
                        previous.is_some() && interval >= tuning.momentum_gap.as_secs_f32();
                    if delta.y != px(0.) {
                        state.input_velocity = delta.y.as_f32() / interval.max(1. / 240.);
                    } else if paused {
                        state.input_velocity = 0.;
                    }
                    let reversed = state.suppressed_direction.is_some_and(|direction| {
                        delta.y != px(0.) && delta.y.as_f32().signum() != direction
                    });
                    if paused || reversed {
                        state.physics.suppress_momentum = false;
                    }
                    if state.physics.suppress_momentum {
                        cx.stop_propagation();
                        return;
                    }
                    if state.physics.offset() != 0. {
                        // Outside a gesture (a phaseless wheel once suppression
                        // lifts) a packet grabs the returning edge and lets it
                        // go again, as it would at rest.
                        let from_rest = !state.physics.dragging;
                        if from_rest {
                            state.physics.begin(bounds.size.height.as_f32());
                        }
                        let remainder = state.physics.pull(delta.y.as_f32());
                        if remainder != 0. {
                            let max = max_scroll_extent(handle.as_ref());
                            let mut offset = handle.offset();
                            offset.y = px(before + remainder).clamp(-max, px(0.));
                            handle.set_offset(offset);
                            scrolled = true;
                        }
                        if ended || from_rest {
                            let velocity = state.input_velocity;
                            state.release_input(from_rest, velocity);
                        }
                        changed = true;
                        cx.stop_propagation();
                    } else if ended {
                        let velocity = state.input_velocity;
                        state.release_input(false, velocity);
                    }
                    if suppress_short_drag_momentum {
                        state.physics.suppress_momentum = true;
                        state.suppressed_direction = None;
                    }
                } else {
                    // Div applies deltas immediately but clamps during its next
                    // prepaint. Clamp here so that boundary deltas are not
                    // mistaken for consumed scrolling (ListState clamps eagerly).
                    let mut offset = handle.offset();
                    let max = max_scroll_extent(handle.as_ref());
                    let clamped = offset.y.clamp(-max, px(0.));
                    if clamped != offset.y {
                        offset.y = clamped;
                        handle.set_offset(offset);
                    }
                    let after = offset.y.as_f32();
                    let requested = delta.y.as_f32();
                    // A List can coalesce several packets against one painted
                    // scroll position. Their offset difference alone does not
                    // prove overscroll, especially after direction changes or
                    // a zero-delta Ended packet from a trackpad.
                    let at_outward_edge = (requested > 0. && offset.y == px(0.))
                        || (requested < 0. && offset.y == -max);
                    let residual =
                        (requested - (after - before)).clamp(requested.min(0.), requested.max(0.));
                    if at_outward_edge
                        && residual.abs() > tuning.residual_epsilon
                        && (!state.physics.suppress_momentum || allow_end_bounce)
                    {
                        let dragging = state.physics.dragging;
                        if !dragging {
                            state.physics.begin(bounds.size.height.as_f32());
                        }
                        state
                            .physics
                            .pull(residual * if dragging { 1. } else { tuning.momentum_gain });
                        if !dragging || ended {
                            let velocity = state.input_velocity;
                            state.release_input(!dragging, velocity);
                        }
                        changed = true;
                    }
                }
                if changed {
                    state.sampled_at = Some(Instant::now());
                }
                drop(state);
                if changed {
                    cx.notify(view);
                }
                if scrolled && let Some(handler) = &on_scroll {
                    handler(window, cx);
                }
            });
        }
        window.with_content_mask(
            Some(ContentMask {
                bounds,
                ..Default::default()
            }),
            |window| {
                self.child.paint(window, cx);
                if let Some((renderer, params)) = &self.deformation {
                    let offset = prepaint.state.borrow().physics.offset();
                    renderer.paint(window, bounds.dilate(px(-1.)), *params, offset);
                }
            },
        );
    }
}

#[derive(Default)]
struct Physics {
    position: f32,
    velocity: f32,
    dragging: bool,
    suppress_momentum: bool,
    extent: f32,
    motion: ScrollBounceMotion,
    tuning: ScrollBounceTuning,
}

impl Physics {
    fn offset(&self) -> f32 {
        if self.dragging {
            let d = self.extent.max(1.);
            let tracking = self.motion.tracking;
            let offset = self.position * tracking / (1. + tracking * self.position.abs() / d);
            if self.tuning.max_bounce > 0. {
                offset.clamp(-self.tuning.max_bounce, self.tuning.max_bounce)
            } else {
                offset
            }
        } else {
            self.position
        }
    }

    fn begin(&mut self, extent: f32) {
        let offset = self.offset();
        // A displaced edge keeps the extent it was stretched under. The
        // rubber-band curve saturates at the extent, so re-reading a viewport
        // that shrank mid-return (rotation, keyboard) could not place the
        // finger where the edge is: it would snap, then need a long pull back.
        if offset == 0. {
            self.extent = (extent * self.tuning.extent_ratio).max(1.);
            if self.tuning.max_bounce > 0. {
                self.extent = self.extent.min(self.tuning.max_bounce);
            }
        }
        // Invert the rubber-band curve so grabbing a returning edge is continuous.
        let tracking = self.motion.tracking;
        self.position = offset
            / (tracking * (1. - offset.abs() / self.extent).max(self.tuning.inverse_epsilon));
        self.velocity = 0.;
        self.dragging = true;
        self.suppress_momentum = false;
    }

    /// Apply finger displacement. Return the part that crosses back into the list.
    fn pull(&mut self, delta: f32) -> f32 {
        let previous = self.position;
        let next = previous + delta;
        if previous != 0. && previous.signum() != next.signum() {
            self.position = 0.;
            next
        } else {
            self.position = next;
            0.
        }
    }

    fn release(&mut self) {
        self.position = self.offset();
        self.dragging = false;
        if self.position != 0. {
            self.suppress_momentum = true;
        }
    }

    /// Exact damped spring integration, independent of refresh rate.
    fn step(&mut self, seconds: f32) -> bool {
        if self.dragging || self.position == 0. && self.velocity == 0. {
            return false;
        }
        let Some(omega) = self.motion.omega() else {
            self.position = 0.;
            self.velocity = 0.;
            return false;
        };
        let zeta = self.motion.damping_ratio;
        let x = self.position;
        let v = self.velocity;
        if (zeta - 1.).abs() < 0.0001 {
            let decay = (-omega * seconds).exp();
            let c = v + omega * x;
            self.position = (x + c * seconds) * decay;
            self.velocity = (v - omega * c * seconds) * decay;
        } else if zeta < 1. {
            let a = zeta * omega;
            let b = omega * (1. - zeta * zeta).sqrt();
            let (sin, cos) = (b * seconds).sin_cos();
            let c = (v + a * x) / b;
            let decay = (-a * seconds).exp();
            self.position = decay * (x * cos + c * sin);
            self.velocity = decay * (v * cos - (a * c + b * x) * sin);
        } else {
            let root = (zeta * zeta - 1.).sqrt();
            let r1 = -omega / (zeta + root);
            let r2 = -omega * (zeta + root);
            let c1 = (v - r2 * x) / (r1 - r2);
            let c2 = x - c1;
            let e1 = (r1 * seconds).exp();
            let e2 = (r2 * seconds).exp();
            self.position = c1 * e1 + c2 * e2;
            self.velocity = r1 * c1 * e1 + r2 * c2 * e2;
        }
        if self.tuning.max_bounce > 0. && self.position.abs() > self.tuning.max_bounce {
            self.position = self.position.signum() * self.tuning.max_bounce;
            if self.velocity.signum() == self.position.signum() {
                self.velocity = 0.;
            }
        }
        if self.position.abs() < self.tuning.settle_position
            && self.velocity.abs() < self.tuning.settle_velocity
        {
            self.position = 0.;
            self.velocity = 0.;
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Context, InteractiveElement as _, ParentElement as _, Render, ScrollHandle,
        StatefulInteractiveElement as _, Styled as _, TestAppContext, VisualTestContext, div,
    };

    struct ScrollTest {
        handle: ScrollHandle,
        enabled: bool,
    }

    impl Render for ScrollTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().p(px(20.)).child(
                ScrollBounce::new(
                    "bounce",
                    &self.handle,
                    div()
                        .id("viewport")
                        .w(px(200.))
                        .h(px(200.))
                        .overflow_y_scroll()
                        .track_scroll(&self.handle)
                        .child(div().h(px(600.)).w_full()),
                )
                .enabled(self.enabled),
            )
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn scroll(cx: &mut VisualTestContext, delta: f32, phase: TouchPhase) {
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(100.), px(100.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
            touch_phase: phase,
            ..Default::default()
        });
    }

    struct ListTest(gpui::ListState);

    impl Render for ListTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            ScrollBounce::new(
                "bounce-list",
                &self.0,
                gpui::list(self.0.clone(), |_, _, _| {
                    div().h(px(40.)).into_any_element()
                })
                .w(px(200.))
                .h(px(200.)),
            )
            .enabled(true)
        }
    }

    #[gpui::test]
    fn list_reverse_drag_preserves_events_before_the_next_frame(cx: &mut TestAppContext) {
        let handle = gpui::ListState::new(30, gpui::ListAlignment::Top, px(0.)).measure_all();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ListTest(handle)
        });
        draw(cx);
        scroll(cx, 100., TouchPhase::Started);
        scroll(cx, -140., TouchPhase::Moved);
        assert_eq!(handle.offset().y, px(-40.));
        scroll(cx, -10., TouchPhase::Moved);
        draw(cx);
        assert_eq!(handle.offset().y, px(-50.));
    }

    #[gpui::test]
    fn trackpad_release_between_frames_does_not_bounce_in_the_middle(cx: &mut TestAppContext) {
        let handle = gpui::ListState::new(30, gpui::ListAlignment::Top, px(0.)).measure_all();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ListTest(handle)
        });
        draw(cx);
        handle.set_offset(point(px(0.), px(-400.)));
        draw(cx);
        let origin = handle.viewport_bounds().origin.y;
        // Unlike simulate_event (which may draw after each event), dispatch
        // both packets within one update to exercise native input coalescing.
        cx.update(|window, cx| {
            for (delta, phase) in [(-30., TouchPhase::Started), (0., TouchPhase::Ended)] {
                window.dispatch_event(
                    gpui::PlatformInput::ScrollWheel(ScrollWheelEvent {
                        position: point(px(100.), px(100.)),
                        delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
                        touch_phase: phase,
                        ..Default::default()
                    }),
                    cx,
                );
            }
        });
        draw(cx);
        assert_eq!(handle.viewport_bounds().origin.y, origin);
        assert!(handle.offset().y < px(-300.) && handle.offset().y > px(-500.));
    }

    #[gpui::test]
    fn trackpad_direction_change_between_frames_does_not_bounce_in_the_middle(
        cx: &mut TestAppContext,
    ) {
        let handle = gpui::ListState::new(30, gpui::ListAlignment::Top, px(0.)).measure_all();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ListTest(handle)
        });
        draw(cx);
        handle.set_offset(point(px(0.), px(-400.)));
        draw(cx);
        let origin = handle.viewport_bounds().origin.y;
        cx.update(|window, cx| {
            for (delta, phase) in [(-30., TouchPhase::Started), (5., TouchPhase::Moved)] {
                window.dispatch_event(
                    gpui::PlatformInput::ScrollWheel(ScrollWheelEvent {
                        position: point(px(100.), px(100.)),
                        delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
                        touch_phase: phase,
                        ..Default::default()
                    }),
                    cx,
                );
            }
        });
        draw(cx);
        assert!(handle.offset().y < px(-300.) && handle.offset().y > px(-500.));
        assert_eq!(handle.viewport_bounds().origin.y, origin);
    }

    #[gpui::test]
    fn list_stretches_only_the_distance_past_either_edge(cx: &mut TestAppContext) {
        for (start, delta, end) in [(-30., 50., 0.), (-970., -50., -1000.)] {
            let mut app = cx.new_app();
            let handle = gpui::ListState::new(30, gpui::ListAlignment::Top, px(0.)).measure_all();
            let (_, cx) = app.add_window_view({
                let handle = handle.clone();
                move |_, _| ListTest(handle)
            });
            draw(cx);
            handle.set_offset(point(px(0.), px(start)));
            draw(cx);
            let origin = handle.viewport_bounds().origin.y;
            scroll(cx, delta, TouchPhase::Started);
            draw(cx);
            assert_eq!(handle.offset().y, px(end));
            let stretch = (handle.viewport_bounds().origin.y - origin).as_f32();
            assert_eq!(stretch.signum(), delta.signum());
            // Of the 50 px input, 30 px is ordinary scrolling. Only the
            // remaining 20 px may be rubber-banded (resistance reduces it).
            assert!(stretch.abs() > 0. && stretch.abs() < 20.);
        }
    }

    #[gpui::test]
    fn reverse_drag_consumes_stretch_before_scrolling_content(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ScrollTest {
                handle,
                enabled: true,
            }
        });
        draw(cx);
        let origin = handle.bounds().origin.y;
        assert_eq!(origin, px(20.));
        scroll(cx, 100., TouchPhase::Started);
        draw(cx);
        assert_eq!(handle.offset().y, px(0.));
        assert!(handle.bounds().origin.y > origin);
        scroll(cx, -140., TouchPhase::Moved);
        draw(cx);
        assert_eq!(handle.offset().y, px(-40.));
        assert_eq!(handle.bounds().origin.y, origin);
    }

    #[gpui::test]
    fn touch_release_ignores_momentum_until_a_new_touch_takes_over(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ScrollTest {
                handle,
                enabled: true,
            }
        });
        draw(cx);
        let origin = handle.bounds().origin.y;
        scroll(cx, 100., TouchPhase::Started);
        draw(cx);
        assert!(handle.bounds().origin.y > origin);
        scroll(cx, 0., TouchPhase::Ended);
        draw(cx);
        let released = handle.bounds().origin.y;
        // The iOS backend emits Moved packets for momentum after finger-up.
        // Even a large inward packet must not move the logical list while
        // the returning edge owns this gesture.
        scroll(cx, -400., TouchPhase::Moved);
        draw(cx);
        assert_eq!(handle.offset().y, px(0.));
        assert!(handle.bounds().origin.y <= released);
        // A new finger-down must end suppression and take over immediately.
        scroll(cx, 0., TouchPhase::Started);
        scroll(cx, -250., TouchPhase::Moved);
        draw(cx);
        assert!(handle.offset().y < px(0.));
        assert_eq!(handle.bounds().origin.y, origin);
    }

    #[gpui::test]
    fn phaseless_wheel_scrolls_back_right_after_bouncing(cx: &mut TestAppContext) {
        let handle = gpui::ListState::new(30, gpui::ListAlignment::Top, px(0.)).measure_all();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ListTest(handle)
        });
        draw(cx);
        handle.set_offset(point(px(0.), px(-1000.)));
        draw(cx);
        let origin = handle.viewport_bounds().origin.y;
        // Smooth-scrolling mouse drivers on macOS: precise deltas, no phase.
        scroll(cx, -50., TouchPhase::Moved);
        draw(cx);
        assert!(handle.viewport_bounds().origin.y < origin);
        scroll(cx, 200., TouchPhase::Moved);
        draw(cx);
        assert!(handle.offset().y > px(-1000.));
        assert_eq!(handle.viewport_bounds().origin.y, origin);
    }

    #[gpui::test]
    fn phaseless_wheel_bounces_again_only_after_a_pause(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ScrollTest {
                handle,
                enabled: true,
            }
        });
        draw(cx);
        scroll(cx, 50., TouchPhase::Moved);
        draw(cx);
        let bounced = handle.bounds().origin.y;
        // Momentum after an edge hit keeps pushing outward; it must not
        // stretch further.
        scroll(cx, 50., TouchPhase::Moved);
        draw(cx);
        assert!(handle.bounds().origin.y <= bounced);
        cx.executor().advance_clock(MOMENTUM_GAP);
        scroll(cx, 50., TouchPhase::Moved);
        draw(cx);
        assert!(handle.bounds().origin.y > bounced);
        assert_eq!(handle.offset().y, px(0.));
    }

    #[gpui::test]
    fn tiny_drag_catching_momentum_does_not_start_a_reverse_fling(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ScrollTest {
                handle,
                enabled: true,
            }
        });
        draw(cx);
        handle.set_offset(point(px(0.), px(-200.)));
        draw(cx);

        // GPUI ends the old momentum stream, then starts a drag at zero
        // displacement when a finger catches the moving content.
        scroll(cx, -60., TouchPhase::Started);
        scroll(cx, 0., TouchPhase::Ended);
        draw(cx);
        scroll(cx, -100., TouchPhase::Moved);
        draw(cx);
        scroll(cx, 0., TouchPhase::Ended);
        draw(cx);
        let before_catch = handle.offset().y;
        assert!(before_catch < px(0.));
        scroll(cx, 0., TouchPhase::Started);
        scroll(cx, 8., TouchPhase::Moved);
        scroll(cx, 0., TouchPhase::Ended);
        draw(cx);
        let stopped = handle.offset().y;
        assert_eq!(stopped, before_catch + px(8.));

        // The recognizer can synthesize a large reverse momentum packet from
        // that 8 px movement. It must not move the logical viewport.
        scroll(cx, 100., TouchPhase::Moved);
        draw(cx);
        assert_eq!(handle.offset().y, stopped);

        // A fresh gesture restores ordinary scrolling and momentum.
        scroll(cx, -20., TouchPhase::Started);
        scroll(cx, 0., TouchPhase::Ended);
        scroll(cx, -10., TouchPhase::Moved);
        draw(cx);
        assert_eq!(handle.offset().y, stopped - px(30.));
    }

    #[gpui::test]
    fn deliberate_drag_after_catching_momentum_can_fling(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ScrollTest {
                handle,
                enabled: true,
            }
        });
        draw(cx);
        handle.set_offset(point(px(0.), px(-200.)));
        draw(cx);

        scroll(cx, 0., TouchPhase::Started);
        scroll(cx, 24., TouchPhase::Moved);
        scroll(cx, 0., TouchPhase::Ended);
        draw(cx);
        assert_eq!(handle.offset().y, px(-176.));
        scroll(cx, 40., TouchPhase::Moved);
        draw(cx);
        assert_eq!(handle.offset().y, px(-136.));
    }

    #[gpui::test]
    fn short_catch_release_still_stretches_past_the_edge(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ScrollTest {
                handle,
                enabled: true,
            }
        });
        draw(cx);
        handle.set_offset(point(px(0.), px(-4.)));
        draw(cx);
        let origin = handle.bounds().origin.y;

        scroll(cx, 0., TouchPhase::Started);
        scroll(cx, 8., TouchPhase::Ended);
        draw(cx);
        assert_eq!(handle.offset().y, px(0.));
        assert!(handle.bounds().origin.y > origin);
    }

    #[gpui::test]
    fn disabled_and_reduced_motion_leave_the_viewport_fixed(cx: &mut TestAppContext) {
        for enabled in [false, true] {
            let mut app = cx.new_app();
            if enabled {
                app.update(|cx| cx.set_reduce_motion(true));
            }
            let handle = ScrollHandle::new();
            let (_, cx) = app.add_window_view({
                let handle = handle.clone();
                move |_, _| ScrollTest { handle, enabled }
            });
            draw(cx);
            let origin = handle.bounds().origin.y;
            scroll(cx, 100., TouchPhase::Started);
            draw(cx);
            assert_eq!(handle.bounds().origin.y, origin);
            scroll(cx, -40., TouchPhase::Moved);
            draw(cx);
            assert_eq!(handle.offset().y, px(-40.));
        }
    }

    #[test]
    fn resistance_and_reverse_preserve_unconsumed_distance() {
        let mut scroll = Physics::default();
        scroll.begin(600.);
        assert_eq!(scroll.pull(100.), 0.);
        assert!(scroll.offset() > 0. && scroll.offset() < 55.);
        assert_eq!(scroll.pull(-130.), -30.);
        assert_eq!(scroll.offset(), 0.);
    }

    #[gpui::test]
    fn diagonal_wobble_stays_with_the_stretch(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ScrollTest {
                handle,
                enabled: true,
            }
        });
        draw(cx);
        let origin = handle.bounds().origin.y;
        scroll(cx, 100., TouchPhase::Started);
        draw(cx);
        let stretched = handle.bounds().origin.y;
        assert!(stretched > origin);
        // A trackpad swipe that started vertical wobbles horizontal-dominant
        // for a packet. Dispatch within one update so the packets stay within
        // the axis lock's gesture separation.
        cx.update(|window, cx| {
            for delta in [point(px(30.), px(-20.)), point(px(0.), px(-20.))] {
                window.dispatch_event(
                    gpui::PlatformInput::ScrollWheel(ScrollWheelEvent {
                        position: point(px(100.), px(100.)),
                        delta: ScrollDelta::Pixels(delta),
                        touch_phase: TouchPhase::Moved,
                        ..Default::default()
                    }),
                    cx,
                );
            }
        });
        draw(cx);
        // Both packets shrink the stretch; neither scrolls the list.
        assert!(handle.bounds().origin.y < stretched);
        assert!(handle.bounds().origin.y > origin);
        assert_eq!(handle.offset(), point(px(0.), px(0.)));
    }

    #[test]
    fn motion_builder_configures_tracking_and_response() {
        let motion = ScrollBounceMotion::default()
            .with_tracking(0.4)
            .with_response(Duration::from_millis(300));
        assert_eq!(motion.tracking(), 0.4);
        assert_eq!(motion.response(), Duration::from_millis(300));
    }

    #[test]
    fn tracking_scales_the_first_stretch() {
        let stretch = |tracking: f32| {
            let mut scroll = Physics {
                motion: ScrollBounceMotion::default().with_tracking(tracking),
                ..Physics::default()
            };
            scroll.begin(600.);
            scroll.pull(100.);
            scroll.offset()
        };
        assert!(stretch(0.3) < stretch(0.55));
        assert!(stretch(0.55) < stretch(0.8));
    }

    #[test]
    fn response_scales_the_return_and_zero_snaps() {
        let remaining = |response: Duration| {
            let mut scroll = Physics {
                motion: ScrollBounceMotion::default().with_response(response),
                ..Physics::default()
            };
            scroll.begin(600.);
            scroll.pull(180.);
            scroll.release();
            scroll.step(0.25);
            scroll.offset()
        };
        assert!(remaining(Duration::from_secs(1)) > remaining(Duration::from_millis(524)));
        assert!(remaining(Duration::from_millis(524)) > remaining(Duration::from_millis(200)));
        assert_eq!(remaining(Duration::ZERO), 0.);
    }

    #[test]
    fn regrabbing_a_displaced_edge_keeps_its_extent() {
        let mut scroll = Physics::default();
        scroll.begin(600.);
        scroll.pull(-150.);
        scroll.release();
        scroll.step(0.08);
        let before = scroll.offset();
        // The viewport shrank below the displacement while the edge was
        // returning. The finger still lands on the edge where it is.
        scroll.begin(40.);
        assert!((scroll.offset() - before).abs() < 0.001);
        // And a short pull inward moves the edge right away.
        scroll.pull(10.);
        assert!(scroll.offset() > before);
        assert!(scroll.offset() < before + 10.);
    }

    #[test]
    fn a_gesture_from_rest_adopts_the_current_extent() {
        let mut scroll = Physics::default();
        scroll.begin(600.);
        scroll.begin(40.);
        scroll.pull(-100.);
        // The stretch saturates below the 40 px viewport, not the 600 px one.
        assert!(scroll.offset() > -40.);
    }

    #[test]
    fn grabbing_the_spring_does_not_jump() {
        let mut scroll = Physics::default();
        scroll.begin(600.);
        scroll.pull(-150.);
        scroll.release();
        scroll.step(0.08);
        let before = scroll.offset();
        scroll.begin(600.);
        assert!((scroll.offset() - before).abs() < 0.001);
        let held = scroll.offset();
        assert!(!scroll.step(0.1));
        assert_eq!(scroll.offset(), held);
    }

    #[test]
    fn spring_has_the_same_trajectory_at_60_and_120_hz() {
        let at = |hz: usize| {
            let mut scroll = Physics::default();
            scroll.begin(600.);
            scroll.pull(180.);
            scroll.release();
            for _ in 0..hz / 4 {
                scroll.step(1. / hz as f32);
            }
            scroll.offset()
        };
        assert!((at(60) - at(120)).abs() < 0.001);
    }

    #[test]
    fn return_keeps_a_visible_tail_after_a_quarter_second() {
        let mut scroll = Physics {
            position: 100.,
            ..Physics::default()
        };
        assert!(scroll.step(0.25));
        // A 100 px release should still have a visible, decelerating tail
        // after 250 ms instead of snapping almost completely back by then.
        assert!(scroll.offset() > 10. && scroll.offset() < 30.);
        assert!(scroll.velocity < 0.);
    }

    #[test]
    fn spring_settles_without_crossing_the_boundary() {
        let mut scroll = Physics::default();
        scroll.begin(600.);
        scroll.pull(-200.);
        scroll.release();
        let mut previous = scroll.offset();
        for _ in 0..120 {
            scroll.step(1. / 120.);
            assert!(scroll.offset() >= previous && scroll.offset() <= 0.);
            previous = scroll.offset();
        }
        assert_eq!(scroll.offset(), 0.);
        assert!(scroll.suppress_momentum);
        scroll.begin(600.);
        assert!(!scroll.suppress_momentum);
    }

    // Demo integration regressions (in addition to the upstream tests above).
    struct OverlayTest {
        handle: ScrollHandle,
        chrome: Rc<std::cell::Cell<Bounds<Pixels>>>,
    }

    impl Render for OverlayTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let chrome = self.chrome.clone();
            div()
                .relative()
                .w(px(600.))
                .h(px(400.))
                .overflow_hidden()
                .child(
                    ScrollBounce::new(
                        "absolute-bounce",
                        &self.handle,
                        div()
                            .id("absolute-viewport")
                            .absolute()
                            .inset_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.handle)
                            .child(div().h(px(1200.)).w_full()),
                    )
                    .enabled(true),
                )
                .child(
                    gpui::canvas(move |bounds, _, _| chrome.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .top(px(20.))
                        .left(px(20.))
                        .w(px(120.))
                        .h(px(44.)),
                )
        }
    }

    #[gpui::test]
    fn absolute_viewport_bounces_beneath_fixed_overlay(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let chrome = Rc::new(std::cell::Cell::new(Bounds::default()));
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            let chrome = chrome.clone();
            move |_, _| OverlayTest { handle, chrome }
        });
        draw(cx);
        assert_eq!(handle.bounds().size.height, px(400.));
        assert_eq!(handle.max_offset().y, px(800.));
        let fixed_bounds = chrome.get();
        scroll(cx, 100., TouchPhase::Started);
        draw(cx);
        assert!(handle.bounds().origin.y > px(0.));
        assert_eq!(chrome.get(), fixed_bounds);
        assert_eq!(handle.offset().y, px(0.));
        scroll(cx, -140., TouchPhase::Moved);
        draw(cx);
        assert_eq!(handle.bounds().origin.y, px(0.));
        assert_eq!(handle.offset().y, px(-40.));
        assert_eq!(chrome.get(), fixed_bounds);
    }

    #[gpui::test]
    fn line_wheel_keeps_normal_scrolling_without_displacement(cx: &mut TestAppContext) {
        let handle = ScrollHandle::new();
        let (_, cx) = cx.add_window_view({
            let handle = handle.clone();
            move |_, _| ScrollTest {
                handle,
                enabled: true,
            }
        });
        draw(cx);
        let origin = handle.bounds().origin.y;
        for delta in [3., -3.] {
            cx.simulate_event(ScrollWheelEvent {
                position: point(px(100.), px(100.)),
                delta: ScrollDelta::Lines(point(0., delta)),
                ..Default::default()
            });
            draw(cx);
            assert_eq!(handle.bounds().origin.y, origin);
        }
        assert!(handle.offset().y < px(0.));
    }
}
