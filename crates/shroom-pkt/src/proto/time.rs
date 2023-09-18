use std::{fmt::Debug, time::Duration};

use chrono::{DateTime, Utc};

use crate::{util::filetime::FileTime, PacketResult};

use super::wrapped::{PacketTryWrapped, PacketWrapped};

/// Represents ticks from the win32 API `GetTickCount`
#[derive(Debug)]
pub struct Ticks(pub u32);

impl PacketWrapped for Ticks {
    type Inner = u32;

    fn packet_into_inner(&self) -> Self::Inner {
        self.0
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self(v)
    }
}

/// Represents time from the win32 API `timeGetTime`
/// time since system start in seconds
#[derive(Debug, Default, Clone)]
pub struct ClientTime(pub u32);

impl ClientTime {
    pub fn as_duration(&self) -> Duration {
        Duration::from_millis(self.0.into())
    }
}

impl PacketWrapped for ClientTime {
    type Inner = u32;

    fn packet_into_inner(&self) -> Self::Inner {
        self.0
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self(v)
    }
}

/// Represents an offset in terms
/// of ms relative to the client time
/// For example -1000 results on the client side to:
/// timeGetTime() - 1000
#[derive(Debug, Clone)]
pub struct ClientTimeOffset(pub DurationMs<i32>);

impl PacketWrapped for ClientTimeOffset {
    type Inner = (bool, u32);

    fn packet_into_inner(&self) -> Self::Inner {
        let v = self.0 .0;
        (v >= 0, v.unsigned_abs())
    }
    fn packet_from(v: Self::Inner) -> Self {
        if v.0 {
            Self(DurationMs(-(v.1 as i32)))
        } else {
            Self(DurationMs(v.1 as i32))
        }
    }
}

/// Timestamps in the protocol
pub type ShroomTime = FileTime;

/// Valid range for the time
pub const SHROOM_TIME_MIN: FileTime = FileTime::from_i64(94354848000000000); // 1/1/1900
pub const SHROOM_TIME_MAX: FileTime = FileTime::from_i64(150842304000000000); // 1/1/2079

impl ShroomTime {
    pub fn is_min(&self) -> bool {
        self == &SHROOM_TIME_MIN
    }

    pub fn is_max(&self) -> bool {
        self == &SHROOM_TIME_MAX
    }

    pub const fn max() -> Self {
        SHROOM_TIME_MAX
    }

    pub const fn min() -> Self {
        SHROOM_TIME_MIN
    }
}

// Encode/Decode helper
impl PacketTryWrapped for ShroomTime {
    type Inner = i64;

    fn packet_into_inner(&self) -> Self::Inner {
        self.filetime()
    }

    fn packet_try_from(v: Self::Inner) -> PacketResult<Self> {
        // Negative timestamp is invalid
        // TODO check min_max range?
        if v < 0 {
            return Err(crate::Error::InvalidTimestamp(v));
        }

        Ok(ShroomTime::from_i64(v))
    }
}

/// Expiration time, can be either None or a time
#[derive(Debug, PartialEq, PartialOrd, Copy, Clone)]
pub struct ShroomExpirationTime(pub Option<ShroomTime>);

impl From<DateTime<Utc>> for ShroomExpirationTime {
    fn from(value: DateTime<Utc>) -> Self {
        Self(Some(value.into()))
    }
}

impl From<Option<DateTime<Utc>>> for ShroomExpirationTime {
    fn from(value: Option<DateTime<Utc>>) -> Self {
        value.into()
    }
}

impl From<ShroomTime> for ShroomExpirationTime {
    fn from(value: ShroomTime) -> Self {
        Self(Some(value))
    }
}

impl From<Option<ShroomTime>> for ShroomExpirationTime {
    fn from(value: Option<ShroomTime>) -> Self {
        Self(value)
    }
}

impl ShroomExpirationTime {
    /// Create expiration from Shroom Time
    pub fn new(time: ShroomTime) -> Self {
        Self(Some(time))
    }

    /// Never expires
    pub fn never() -> Self {
        Self(None)
    }

    /// Create a delayed expiration from now + the duration
    pub fn delay(dur: chrono::Duration) -> Self {
        (Utc::now() + dur).into()
    }
}

impl PacketWrapped for ShroomExpirationTime {
    type Inner = ShroomTime;

    fn packet_into_inner(&self) -> Self::Inner {
        self.0.unwrap_or(SHROOM_TIME_MAX)
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self((v != SHROOM_TIME_MAX).then_some(v))
    }
}

/// Represents a Duration in ms with the backed type
#[derive(Clone, Copy, PartialEq)]
pub struct DurationMs<T>(pub T);

impl<T: Debug> Debug for DurationMs<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}ms", self.0)
    }
}

impl<T> PacketWrapped for DurationMs<T>
where
    T: Copy,
{
    type Inner = T;

    fn packet_into_inner(&self) -> Self::Inner {
        self.0
    }

    fn packet_from(v: Self::Inner) -> Self {
        Self(v)
    }
}

/// Convert a `Duration` into this MS duration type
impl<T> From<Duration> for DurationMs<T>
where
    T: TryFrom<u128>,
    T::Error: Debug,
{
    fn from(value: Duration) -> Self {
        Self(T::try_from(value.as_millis()).expect("Milli conversion"))
    }
}

/// Convert a DurationMS into a `Duration`
impl<T> From<DurationMs<T>> for Duration
where
    T: Into<u64>,
{
    fn from(value: DurationMs<T>) -> Self {
        Duration::from_millis(value.0.into())
    }
}

/// Duration ins ms, backed by u16
pub type ShroomDurationMs16 = DurationMs<u16>;
/// Duration in ms, backed by u32
pub type ShroomDurationMs32 = DurationMs<u32>;

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use std::time::Duration;

    use crate::{
        test_util::{test_enc_dec, test_enc_dec_all},
        time::{ShroomDurationMs16, ShroomDurationMs32},
    };

    use super::{DurationMs, ShroomExpirationTime, ShroomTime};

    proptest! {
        #[test]
        fn q_dur16(dur: u16) {
            let dur = Duration::from_millis(dur.into());
            test_enc_dec::<ShroomDurationMs16>(dur.into());
        }

        #[test]
        fn q_dur32(dur: u32) {
            let dur = Duration::from_millis(dur.into());
            test_enc_dec::<ShroomDurationMs32>(dur.into());
        }
    }

    #[test]
    fn dur() {
        test_enc_dec_all([
            DurationMs::<u32>(1),
            Duration::from_millis(100 as u64).into(),
        ]);
    }

    #[test]
    fn expiration_time() {
        test_enc_dec_all([
            ShroomExpirationTime::never(),
            ShroomExpirationTime(None),
            ShroomExpirationTime::delay(chrono::Duration::seconds(1_000)),
            ShroomExpirationTime::new(ShroomTime::now()),
        ]);
    }
}
