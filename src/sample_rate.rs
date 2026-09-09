use std::f32;

pub(crate) const MAX_SAMPLE_DELAY: u16 = 6 * 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BaseClock {
    Clk30Mhz,
    Clk48Mhz,
}

impl BaseClock {
    fn as_hz(self) -> f32 {
        match self {
            BaseClock::Clk30Mhz => 30_000_000.0,
            BaseClock::Clk48Mhz => 48_000_000.0,
        }
    }
}

/// A sample rate supported by fx2lafw
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleRate {
    pub(crate) base: BaseClock,
    pub(crate) divisor: u16,
}

impl SampleRate {
    /// Find the closest supported `SampleRate` to the given value in Hz.
    pub fn from_hz(sample_rate: f32) -> Self {
        // `if` rather than `clamp` to handle NaN
        let sample_rate = if sample_rate > 1.0 { sample_rate } else { 1.0 }.min(BaseClock::Clk30Mhz.as_hz());

        let with_base = |base: BaseClock| {
            let divisor = (base.as_hz() / sample_rate).round().clamp(1.0, MAX_SAMPLE_DELAY as f32) as u16;
            SampleRate { base, divisor }
        };

        let c30 = with_base(BaseClock::Clk30Mhz);
        let c48 = with_base(BaseClock::Clk48Mhz);

        if (c30.as_hz() - sample_rate).abs() < (c48.as_hz() - sample_rate).abs() { c30 } else { c48 }
    }

    /// Get the rate in Hz
    pub fn as_hz(&self) -> f32 {
        self.base.as_hz() / (self.divisor as f32)
    }

    /// Get the rate in Hz, expressed as a (numerator, denominator) fraction
    pub fn as_hz_ratio(&self) -> (u32, u32) {
        (self.base.as_hz() as u32, self.divisor as u32)
    }
}

#[test]
fn test_sample_rate_from_hz() {
    assert_eq!(SampleRate::from_hz(f32::NAN), SampleRate { base: BaseClock::Clk30Mhz, divisor: MAX_SAMPLE_DELAY });
    assert_eq!(SampleRate::from_hz(-1.0), SampleRate { base: BaseClock::Clk30Mhz, divisor: MAX_SAMPLE_DELAY });
    assert_eq!(SampleRate::from_hz(0.0), SampleRate { base: BaseClock::Clk30Mhz, divisor: MAX_SAMPLE_DELAY });
    assert_eq!(SampleRate::from_hz(1.0), SampleRate { base: BaseClock::Clk30Mhz, divisor: MAX_SAMPLE_DELAY });
    assert_eq!(SampleRate::from_hz(1000000.0), SampleRate { base: BaseClock::Clk48Mhz, divisor: 48 });
    assert_eq!(SampleRate::from_hz(1000002.0), SampleRate { base: BaseClock::Clk48Mhz, divisor: 48 });
    assert_eq!(SampleRate::from_hz(4800000.0), SampleRate { base: BaseClock::Clk48Mhz, divisor: 10 });
    assert_eq!(SampleRate::from_hz(15000000.0), SampleRate { base: BaseClock::Clk30Mhz, divisor: 2 });
    assert_eq!(SampleRate::from_hz(25000000.0), SampleRate { base: BaseClock::Clk48Mhz, divisor: 2 });
    assert_eq!(SampleRate::from_hz(30000000.0), SampleRate { base: BaseClock::Clk30Mhz, divisor: 1 });
    assert_eq!(SampleRate::from_hz(50000000.0), SampleRate { base: BaseClock::Clk30Mhz, divisor: 1 });
    assert_eq!(SampleRate::from_hz(f32::INFINITY), SampleRate { base: BaseClock::Clk30Mhz, divisor: 1 });
}

#[test]
fn test_sample_rate_as_hz() {
    assert_eq!(SampleRate { base: BaseClock::Clk48Mhz, divisor: 2 }.as_hz(), 24000000.0);
    assert_eq!(SampleRate { base: BaseClock::Clk48Mhz, divisor: 48 }.as_hz(), 1000000.0);
    assert_eq!(SampleRate { base: BaseClock::Clk30Mhz, divisor: 1 }.as_hz(), 30000000.0);
    assert_eq!(SampleRate { base: BaseClock::Clk30Mhz, divisor: 30 }.as_hz(), 1000000.0);
}
