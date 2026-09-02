use crate::{Pixels, Rems};

const CRITICAL_DAMPING_TOLERANCE: f32 = 1e-4;

/// The physical parameters of a damped harmonic oscillator.
///
/// `stiffness` and `mass` must be finite and positive. `damping` must be finite
/// and non-negative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringConfig {
    /// The spring stiffness, conventionally written as $k$.
    pub stiffness: f32,
    /// The viscous damping coefficient, conventionally written as $c$.
    pub damping: f32,
    /// The moving mass, conventionally written as $m$.
    pub mass: f32,
}

impl SpringConfig {
    /// Creates a spring from its physical parameters.
    pub const fn new(stiffness: f32, damping: f32, mass: f32) -> Self {
        Self {
            stiffness,
            damping,
            mass,
        }
    }

    /// Returns the natural angular frequency and damping ratio.
    pub fn canonical(&self) -> (f32, f32) {
        let natural_frequency = (self.stiffness / self.mass).sqrt();
        let damping_ratio = self.damping / (2.0 * (self.stiffness * self.mass).sqrt());
        (natural_frequency, damping_ratio)
    }

    /// Advances a spring toward a target that remains fixed for `delta_time`.
    ///
    /// This analytic step is independent of frame rate and preserves velocity,
    /// allowing an interrupted spring to be retargeted without restarting it.
    pub fn step(&self, state: SpringState, target: f32, delta_time: f32) -> SpringState {
        let propagator = self.propagator(delta_time);
        let displacement = state.position - target;

        SpringState {
            position: target + propagator[0][0] * displacement + propagator[0][1] * state.velocity,
            velocity: propagator[1][0] * displacement + propagator[1][1] * state.velocity,
        }
    }

    /// Returns the exact state-transition matrix for a constant target.
    ///
    /// A matrix may be shared by springs with the same configuration and frame
    /// delta, but must not be reused when the frame delta changes.
    pub fn propagator(&self, delta_time: f32) -> [[f32; 2]; 2] {
        let (natural_frequency, damping_ratio) = self.canonical();

        if damping_ratio < 1.0 - CRITICAL_DAMPING_TOLERANCE {
            let decay = damping_ratio * natural_frequency;
            let damped_frequency = natural_frequency * (1.0 - damping_ratio * damping_ratio).sqrt();
            let exponential = (-decay * delta_time).exp();
            let (sine, cosine) = (damped_frequency * delta_time).sin_cos();
            let sine_over_frequency = sine / damped_frequency;

            [
                [
                    exponential * (cosine + decay * sine_over_frequency),
                    exponential * sine_over_frequency,
                ],
                [
                    -exponential * natural_frequency * natural_frequency * sine_over_frequency,
                    exponential * (cosine - decay * sine_over_frequency),
                ],
            ]
        } else if damping_ratio > 1.0 + CRITICAL_DAMPING_TOLERANCE {
            let root = (damping_ratio * damping_ratio - 1.0).sqrt();
            let root_sum = damping_ratio + root;
            let slow_root = -natural_frequency / root_sum;
            let fast_root = -natural_frequency * root_sum;
            let denominator = slow_root - fast_root;
            let slow_exponential = (slow_root * delta_time).exp();
            let fast_exponential = (fast_root * delta_time).exp();

            [
                [
                    (-fast_root * slow_exponential + slow_root * fast_exponential) / denominator,
                    (slow_exponential - fast_exponential) / denominator,
                ],
                [
                    slow_root * fast_root * (fast_exponential - slow_exponential) / denominator,
                    (slow_root * slow_exponential - fast_root * fast_exponential) / denominator,
                ],
            ]
        } else {
            let exponential = (-natural_frequency * delta_time).exp();

            [
                [
                    exponential * (1.0 + natural_frequency * delta_time),
                    exponential * delta_time,
                ],
                [
                    -exponential * natural_frequency * natural_frequency * delta_time,
                    exponential * (1.0 - natural_frequency * delta_time),
                ],
            ]
        }
    }

    /// Tests both displacement and velocity against a positional tolerance.
    ///
    /// Velocity is compared with `epsilon * natural_frequency`, giving it the
    /// corresponding animated-units-per-second scale.
    pub fn is_settled(&self, state: SpringState, target: f32, epsilon: f32) -> bool {
        let (natural_frequency, _) = self.canonical();
        epsilon.is_finite()
            && epsilon >= 0.0
            && (state.position - target).abs() <= epsilon
            && state.velocity.abs() <= epsilon * natural_frequency
    }
}

/// The instantaneous position and velocity of a spring.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpringState {
    /// The current value in the animated unit.
    pub position: f32,
    /// The current value's change per second.
    pub velocity: f32,
}

/// A value that can be targeted by a one-dimensional spring.
///
/// Implementations may project the spring coordinate into a richer output,
/// allowing a discrete state or a path through a multidimensional value to be
/// driven by one spring.
pub trait SpringTarget: 'static {
    /// The value produced by resolving a spring coordinate.
    type Output;

    /// Returns the target in the spring's coordinate space.
    fn target(&self) -> f32;

    /// Projects a spring coordinate into the animated output.
    fn resolve(&self, value: f32) -> Self::Output;
}

impl SpringTarget for f32 {
    type Output = f32;

    fn target(&self) -> f32 {
        *self
    }

    fn resolve(&self, value: f32) -> Self::Output {
        value
    }
}

impl SpringTarget for Pixels {
    type Output = Pixels;

    fn target(&self) -> f32 {
        self.as_f32()
    }

    fn resolve(&self, value: f32) -> Self::Output {
        Pixels::from(value)
    }
}

impl SpringTarget for Rems {
    type Output = Rems;

    fn target(&self) -> f32 {
        self.0
    }

    fn resolve(&self, value: f32) -> Self::Output {
        Rems(value)
    }
}

impl SpringTarget for bool {
    type Output = AnimationPhase;

    fn target(&self) -> f32 {
        if *self { 1.0 } else { 0.0 }
    }

    fn resolve(&self, value: f32) -> Self::Output {
        AnimationPhase(value)
    }
}

/// A potentially overshooting coordinate within an animation.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct AnimationPhase(
    /// The unbounded phase coordinate.
    pub f32,
);

impl From<f32> for AnimationPhase {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl From<bool> for AnimationPhase {
    fn from(value: bool) -> Self {
        Self(if value { 1.0 } else { 0.0 })
    }
}

impl SpringTarget for AnimationPhase {
    type Output = AnimationPhase;

    fn target(&self) -> f32 {
        self.0
    }

    fn resolve(&self, value: f32) -> Self::Output {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{px, rems};

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn analytic_step_is_independent_of_frame_partitioning() {
        let config = SpringConfig::new(100.0, 10.0, 1.0);
        let initial = SpringState {
            position: 0.0,
            velocity: 0.0,
        };
        let one_step = config.step(initial, 1.0, 1.0);
        let many_steps = (0..100).fold(initial, |state, _| config.step(state, 1.0, 0.01));

        assert_close(many_steps.position, one_step.position);
        assert_close(many_steps.velocity, one_step.velocity);
    }

    #[test]
    fn settling_requires_both_position_and_velocity_to_be_within_tolerance() {
        let config = SpringConfig::new(100.0, 10.0, 1.0);

        assert!(config.is_settled(
            SpringState {
                position: 0.9995,
                velocity: 0.005,
            },
            1.0,
            0.001,
        ));
        assert!(!config.is_settled(
            SpringState {
                position: 0.9995,
                velocity: 0.02,
            },
            1.0,
            0.001,
        ));
        assert!(!config.is_settled(
            SpringState {
                position: 0.99,
                velocity: 0.0,
            },
            1.0,
            0.001,
        ));
    }

    #[test]
    fn spring_targets_preserve_their_coordinate_units() {
        assert_eq!(0.25_f32.target(), 0.25);
        assert_eq!(0.25_f32.resolve(0.75), 0.75);

        let pixels = px(12.0);
        assert_eq!(pixels.target(), 12.0);
        assert_eq!(pixels.resolve(24.0), px(24.0));

        let rem_value = rems(1.5);
        assert_eq!(rem_value.target(), 1.5);
        assert_eq!(rem_value.resolve(2.0), rems(2.0));

        assert_eq!(false.target(), 0.0);
        assert_eq!(true.target(), 1.0);
        assert_eq!(true.resolve(1.25), AnimationPhase(1.25));
        assert_eq!(AnimationPhase(2.0).resolve(1.5), AnimationPhase(1.5));
    }
}
